//! `rdm` — the command-line frontend. It parses arguments, renders progress,
//! and delegates everything else to the engine. No download logic lives here.

use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context;
use clap::{CommandFactory, Parser};
use indicatif::{ProgressBar, ProgressStyle};
use tracing::Level;
use tracing_subscriber::filter::{filter_fn, FilterExt};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

use rdm::{
    download, engine::state, parse_rate, redact_url, Checksum, Control, DownloadRequest, Error,
    Proxy, ProxyMode, RateLimiter, Summary,
};

/// Printed when Ctrl-C paused the download.
const PAUSED_MESSAGE: &str = "Paused — run the same command again to resume";

/// Exit code after a Ctrl-C (the shell convention for SIGINT, 128 + 2).
const INTERRUPTED: u8 = 130;

/// Exit code after a bug in rdm: Rust's for a panic that ends `main`.
const BUG: u8 = 101;

/// This build's version, for the `-v` startup line and the bug-report hint.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where to report a bug.
const ISSUES_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/issues");

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

    /// Download through this proxy instead of the system's: `http://`,
    /// `https://`, `socks5://` or `socks5h://` (DNS through the proxy), with
    /// an optional `user:pass@`, e.g. `socks5h://127.0.0.1:1080`.
    // Parsed in `main`, not by clap, whose error would repeat the value and
    // with it any password.
    #[arg(long = "proxy", value_name = "URL")]
    proxy: Option<String>,

    /// Connect directly, ignoring the system's proxy (the OS settings and
    /// the `HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY` environment variables).
    #[arg(long = "no-proxy", conflicts_with = "proxy")]
    no_proxy: bool,

    /// Also log informational engine events (download start, fallbacks) to
    /// stderr; `RUST_LOG` overrides the level.
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,
}

/// clap value parser: `sha256:<hex>` → [`Checksum`].
fn parse_checksum(s: &str) -> Result<Checksum, String> {
    s.parse()
}

/// `--proxy` → [`Proxy`], or exit with a usage error that names the flag but
/// not the value.
fn parse_proxy(s: &str) -> Proxy {
    s.parse().unwrap_or_else(|e| {
        Cli::command()
            .error(
                clap::error::ErrorKind::ValueValidation,
                format!("invalid value for '--proxy <URL>': {e}"),
            )
            .exit()
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    install_panic_hook();
    let cli = Cli::parse();

    let mut req = match cli.output {
        Some(out) => DownloadRequest::to_path(cli.url, out),
        None => DownloadRequest::new(cli.url, std::env::current_dir()?),
    }
    .with_connections(cli.connections);
    if let Some(checksum) = cli.checksum {
        req = req.with_checksum(checksum);
    }
    if let Some(proxy) = cli.proxy.as_deref().map(parse_proxy) {
        req = req.with_proxy(proxy);
    } else if cli.no_proxy {
        req = req.with_proxy(ProxyMode::None);
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
    tracing::info!("rdm {VERSION} · {}", rdm::platform::os_description());

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
        // A bug caught inside the engine (the hook has already asked for a
        // report): the same exit code as a panic that ends `main`.
        Err(e @ Error::Internal(_)) => {
            eprintln!("Error: failed to download {}: {e}", redact_url(&req.url));
            Ok(ExitCode::from(BUG))
        }
        Err(e) => Err(e).with_context(|| format!("failed to download {}", redact_url(&req.url))),
    }
}

/// Log a panic (a bug in rdm) as an ERROR event, let the default hook print
/// it, then ask the user to report it. The backtrace is logged at DEBUG, so
/// `RUST_LOG=debug` shows it without `RUST_BACKTRACE`.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info.payload_as_str().unwrap_or("panic with no message");
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_default();
        let thread = std::thread::current();
        let thread = thread.name().unwrap_or("<unnamed>");
        tracing::error!(
            version = VERSION,
            thread,
            location,
            "rdm panicked: {message}"
        );
        if tracing::enabled!(Level::DEBUG) {
            let backtrace = std::backtrace::Backtrace::force_capture();
            tracing::debug!("Panic backtrace:\n{backtrace}");
        }
        default_hook(info);
        eprintln!("{}", bug_report_hint());
    }));
}

/// What the CLI says after a panic.
fn bug_report_hint() -> String {
    format!(
        "rdm {VERSION} hit a bug. Please report it at {ISSUES_URL} with the output \
         above (run again with -v for more detail)."
    )
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
