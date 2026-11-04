//! `rdm` — the command-line frontend. It parses arguments, renders progress,
//! and delegates everything else to the engine. No download logic lives here.

use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use tracing::Level;
use tracing_subscriber::filter::{filter_fn, FilterExt};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

use rdm::{
    download, engine::state, parse_rate, Checksum, Control, DownloadRequest, Error, RateLimiter,
    Summary,
};

/// Printed when Ctrl-C paused the download.
const PAUSED_MESSAGE: &str = "Paused — run the same command again to resume";

/// Exit code after a Ctrl-C (the shell convention for SIGINT, 128 + 2).
const INTERRUPTED: u8 = 130;

/// A download manager in Rust.
#[derive(Parser, Debug)]
#[command(name = "rdm", version, about)]
struct Cli {
    /// URL to download.
    url: String,

    /// Output file path (default: a name derived from the URL/headers, saved
    /// in the current directory).
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    /// Verify the finished file against a checksum, e.g.
    /// `--checksum sha256:<hex>`.
    #[arg(long = "checksum", value_parser = parse_checksum)]
    checksum: Option<Checksum>,

    /// Max parallel connections for a large file on a range-capable server,
    /// added while the measured speed keeps rising (1 = single connection).
    #[arg(
        short = 'c',
        long = "connections",
        default_value_t = 4,
        value_parser = clap::value_parser!(u8).range(1..=16)
    )]
    connections: u8,

    /// Cap the download speed in bytes/s, with an optional K/M/G suffix
    /// (×1024), e.g. `500K` or `2M`.
    #[arg(long = "limit-rate", value_name = "RATE", value_parser = parse_rate)]
    limit_rate: Option<u64>,

    /// Also log informational engine events (download start, fallbacks) to
    /// stderr; `RUST_LOG` overrides the level.
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,
}

/// clap value parser: `sha256:<hex>` → [`Checksum`].
fn parse_checksum(s: &str) -> Result<Checksum, String> {
    s.parse()
}

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();

    let mut req = match cli.output {
        Some(out) => DownloadRequest::to_path(cli.url, out),
        None => DownloadRequest::new(cli.url, std::env::current_dir()?),
    }
    .with_connections(cli.connections);
    if let Some(checksum) = cli.checksum {
        req = req.with_checksum(checksum);
    }

    // When the target is known up front (explicit -o), note whether a prior
    // partial exists so we can tell the user we're resuming. The engine does
    // the actual resume off the `.part` + sidecar; this is only a message.
    if let Some(out) = &req.out {
        let sidecar = state::sidecar_path(out);
        if sidecar.exists() && state::downloaded_bytes(out).is_some() {
            println!("Resuming {}", out.display());
        }
    }

    let bar = ProgressBar::new(0);
    bar.set_style(
        ProgressStyle::with_template(
            "{bar:40.cyan/blue} {bytes}/{total_bytes} ({binary_bytes_per_sec}, eta {eta}) {msg}",
        )
        .expect("valid template")
        .progress_chars("=>-"),
    );
    init_logging(&bar, cli.verbose);

    let limiter = Arc::new(RateLimiter::new(cli.limit_rate));
    let control = Control::new().with_limiters([limiter]);
    pause_on_ctrl_c(control.clone(), bar.clone());

    let result = download(&req, &control, |p| {
        if let Some(total) = p.total {
            bar.set_length(total);
        }
        bar.set_position(p.downloaded);
        bar.set_message(if p.connections > 1 {
            format!("{} conn", p.connections)
        } else {
            String::new()
        });
    })
    .await;
    bar.finish_and_clear();

    match result {
        Ok(summary) => {
            println!("{}", done_message(&summary, req.checksum.as_ref()));
            Ok(ExitCode::SUCCESS)
        }
        Err(Error::Paused) => {
            eprintln!("{PAUSED_MESSAGE}");
            Ok(ExitCode::from(INTERRUPTED))
        }
        Err(e) => Err(e).with_context(|| format!("failed to download {}", req.url)),
    }
}

/// The line printed after a successful download.
fn done_message(summary: &Summary, checksum: Option<&Checksum>) -> String {
    let verified = checksum
        .map(|c| format!(" ({} verified)", c.algo.name()))
        .unwrap_or_default();
    format!(
        "Downloaded {} bytes to {}{verified}",
        summary.bytes,
        summary.path.display()
    )
}

/// First Ctrl-C pauses, so the engine flushes and checkpoints before `download`
/// returns [`Error::Paused`]; a second one exits at once.
fn pause_on_ctrl_c(control: Control, bar: ProgressBar) {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            control.pause();
            if tokio::signal::ctrl_c().await.is_ok() {
                bar.abandon();
                std::process::exit(INTERRUPTED.into());
            }
        }
    });
}

/// Engine events to stderr: warnings by default, info with `-v`, or whatever
/// `RUST_LOG` says.
fn init_logging(bar: &ProgressBar, verbose: bool) {
    let (filter, hide_engine_errors) = match EnvFilter::try_from_default_env() {
        Ok(explicit) => (explicit, false),
        Err(_) => (EnvFilter::new(if verbose { "info" } else { "warn" }), true),
    };
    // The CLI prints the returned error itself; the engine's ERROR line would
    // only repeat it. Events only: the engine's ERROR-level span must survive.
    let not_engine_error = filter_fn(move |meta| {
        !(hide_engine_errors
            && meta.is_event()
            && *meta.level() == Level::ERROR
            && meta.target().starts_with("rdm::engine"))
    });
    let layer = tracing_subscriber::fmt::layer()
        .with_ansi(io::stderr().is_terminal())
        .with_writer(BarWriter(bar.clone()))
        .with_filter(filter.and(not_engine_error));
    let _ = tracing_subscriber::registry().with(layer).try_init();
}

/// Writes log lines to stderr with the progress bar hidden, so a line never
/// lands in the middle of a redraw.
#[derive(Clone)]
struct BarWriter(ProgressBar);

impl<'a> MakeWriter<'a> for BarWriter {
    type Writer = BarWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

impl Write for BarWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.suspend(|| io::stderr().write(buf))
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.0.suspend(|| io::stderr().write_all(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stderr().flush()
    }
}
