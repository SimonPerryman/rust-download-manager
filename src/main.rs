//! `rdm` — the command-line frontend. It parses arguments, renders progress,
//! and delegates everything else to the engine. No download logic lives here.

use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Context;
use clap::builder::TypedValueParser;
use clap::{CommandFactory, Parser};
use indicatif::{ProgressBar, ProgressStyle};
use tracing::Level;
use tracing_subscriber::filter::{filter_fn, FilterExt};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

use rdm::engine::actions::{backend_from_env, PowerAction, POWER_COUNTDOWN_SECS};
use rdm::engine::schedule::{Clock, FakeClock, LocalDateTime, StartAt, SystemClock};
use rdm::{
    download, engine::state, parse_list, parse_rate, redact_url, redact_urls_in, run_limited,
    Checksum, Control, DownloadRequest, Error, Proxy, ProxyMode, RateLimiter, Summary,
    TargetClaims,
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
///
/// Downloads resume: Ctrl-C (or a crash) keeps the partial file as
/// `<file>.part` with a `<file>.rdm` sidecar, and running the same command
/// again continues where it stopped. Large files on servers that support
/// range requests use several connections.
///
/// Exit codes: 0 done, 1 a download failed, 2 invalid arguments, 101 a bug
/// in rdm, 130 paused by Ctrl-C.
#[derive(Parser, Debug)]
#[command(
    name = "rdm",
    version,
    after_help = "\
Examples:
  # download to the current directory (run it again after Ctrl-C to resume)
  rdm https://example.com/big.iso

  # choose the output file
  rdm -o out.zip https://example.com/a.zip

  # 8 connections, capped at 2 MiB/s
  rdm -c 8 --limit-rate 2M https://example.com/a.zip

  # verify the finished file
  rdm --checksum sha256:<hex> https://example.com/a.zip

  # batch from a file, 5 at a time
  rdm -i urls.txt -j 5

  # a numbered range of links
  rdm 'https://example.com/lesson[01-12].mp4'

  # wait until 01:30 (tonight, or tomorrow if it has passed), then download
  rdm --at 01:30 https://example.com/big.iso

  # put the machine to sleep a minute after everything has downloaded
  rdm --on-finish sleep -i urls.txt"
)]
struct Cli {
    /// URL(s) to download. Several links download as a batch.
    #[arg(value_name = "URL", required_unless_present = "input")]
    urls: Vec<String>,

    /// Download every link in this file (`-` reads standard input): one per
    /// line or separated by spaces, `#` comments ignored. Can be combined
    /// with links on the command line.
    #[arg(
        short = 'i',
        long = "input-file",
        value_name = "FILE",
        conflicts_with_all = ["output", "checksum"]
    )]
    input: Option<PathBuf>,

    /// How many links of a batch download at once.
    #[arg(
        short = 'j',
        long = "jobs",
        default_value_t = 3,
        value_parser = clap::value_parser!(u8).range(1..=32)
    )]
    jobs: u8,

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

    /// Wait until this local time, then download: `HH:MM` is the next such
    /// time (tomorrow's if it has passed today), or give a full
    /// `YYYY-MM-DDTHH:MM`. rdm must stay running; Ctrl-C while waiting
    /// cancels. Without it the download starts at once.
    #[arg(long = "at", value_name = "TIME", value_parser = parse_at)]
    at: Option<String>,

    /// After every download has finished, wait 60 seconds (Ctrl-C cancels),
    /// then put the machine to `sleep` or `shutdown`. Not after a failure or
    /// a Ctrl-C.
    #[arg(
        long = "on-finish",
        value_name = "ACTION",
        value_parser = clap::builder::PossibleValuesParser::new(["sleep", "shutdown"])
            .try_map(|s| s.parse::<PowerAction>())
    )]
    on_finish: Option<PowerAction>,

    /// Also log informational engine events (download start, fallbacks) to
    /// stderr; `RUST_LOG` overrides the level.
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,
}

/// clap value parser: `sha256:<hex>` → [`Checksum`].
fn parse_checksum(s: &str) -> Result<Checksum, String> {
    s.parse()
}

/// clap value parser for `--at`: the text if it reads as a start time. It is
/// resolved against the clock only once the wait begins.
fn parse_at(s: &str) -> Result<String, String> {
    let probe = LocalDateTime::from_parts(2000, 1, 1, 0, 0).expect("a real date");
    StartAt::parse(s, probe)
        .map(|_| s.trim().to_string())
        .map_err(|e| e.to_string())
}

/// Wait for `--at`, polling the clock. The `RDM_TEST_NOW` environment
/// variable (`YYYY-MM-DDTHH:MM`) swaps the system clock for one that starts
/// there and moves a minute per poll, so tests don't wait in real time.
async fn wait_for_start(text: &str) {
    let test_now = std::env::var("RDM_TEST_NOW")
        .ok()
        .and_then(|v| v.parse::<LocalDateTime>().ok());
    let fake = test_now.map(FakeClock::new);
    let system = SystemClock::new();
    let clock: &dyn Clock = match &fake {
        Some(fake) => fake,
        None => &system,
    };
    let at = StartAt::parse(text, clock.now()).expect("checked when the arguments were read");
    println!(
        "Waiting until {} to start",
        at.0.to_string().replace('T', " ")
    );
    while !at.is_due(clock.now()) {
        match &fake {
            Some(fake) => {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                fake.advance(1);
            }
            None => tokio::time::sleep(std::time::Duration::from_secs(1)).await,
        }
    }
}

/// Carry out `--on-finish`: count down (Ctrl-C cancels), then ask the power
/// backend. `RDM_TEST_POWER_LOG` (a file) swaps the real machine for a log of
/// what would have been done, and `RDM_TEST_POWER_COUNTDOWN` the seconds, so
/// tests neither wait nor power anything down. Returns the exit code to end
/// with.
async fn power_after(action: Option<PowerAction>) -> ExitCode {
    let Some(action) = action else {
        return ExitCode::SUCCESS;
    };
    let secs = std::env::var("RDM_TEST_POWER_COUNTDOWN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(POWER_COUNTDOWN_SECS);
    println!(
        "Everything is done. {} in {secs} s (Ctrl-C to cancel)",
        action.name()
    );
    tokio::select! {
        () = tokio::time::sleep(std::time::Duration::from_secs(secs)) => {}
        _ = tokio::signal::ctrl_c() => {
            println!("Cancelled");
            return ExitCode::from(INTERRUPTED);
        }
    }
    match backend_from_env().perform(action) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: couldn't {}: {e}", action.name());
            ExitCode::FAILURE
        }
    }
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

    let is_batch = cli.input.is_some() || cli.urls.len() > 1 || is_range_link(&cli.urls[0]);
    if is_batch && (cli.output.is_some() || cli.checksum.is_some()) {
        usage_error("-o and --checksum take a single link, not a batch");
    }
    if let Some(at) = &cli.at {
        wait_for_start(at).await;
    }
    if is_batch {
        return run_batch(cli).await;
    }
    let on_finish = cli.on_finish;
    let url = cli.urls.into_iter().next().expect("clap requires a URL");

    let mut req = match cli.output {
        Some(out) => DownloadRequest::to_path(url, out),
        None => DownloadRequest::new(url, std::env::current_dir()?),
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
            Ok(power_after(on_finish).await)
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

/// Exit with a usage error (code 2) naming what is wrong.
fn usage_error(message: &str) -> ! {
    Cli::command()
        .error(clap::error::ErrorKind::ArgumentConflict, message)
        .exit()
}

/// The batch's links: the `-i` file (or stdin) first, then the command-line
/// links, read by the engine's list rules.
fn batch_list(cli: &Cli) -> anyhow::Result<rdm::ParsedList> {
    let mut text = String::new();
    match cli.input.as_deref() {
        Some(p) if p.as_os_str() == "-" => {
            io::Read::read_to_string(&mut io::stdin(), &mut text)
                .context("failed to read links from standard input")?;
        }
        Some(p) => {
            text = std::fs::read_to_string(p)
                .with_context(|| format!("failed to read {}", p.display()))?;
        }
        None => {}
    }
    for url in &cli.urls {
        text.push('\n');
        text.push_str(url);
    }
    Ok(parse_list(&text)?)
}

/// Whether a lone `<URL>` holds a `[a-b]` range (or a refused one), which
/// makes it a batch.
fn is_range_link(url: &str) -> bool {
    url.contains('[')
        && parse_list(url).is_ok_and(|l| {
            l.links.iter().any(|l| l.pattern.is_some())
                || l.skipped.iter().any(|s| s.reason.is_range_problem())
        })
}

/// Ctrl-C for a batch: stops new items from starting and pauses the running
/// ones, which keep their `.part` to resume on a rerun.
#[derive(Default)]
struct BatchStop {
    stopped: AtomicBool,
    running: Mutex<Vec<Control>>,
}

impl BatchStop {
    /// A control for the next item, or `None` once stopped.
    fn start_item(&self, limit: Option<u64>) -> Option<Control> {
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        if self.stopped.load(Ordering::SeqCst) {
            return None;
        }
        let control = Control::new().with_limiters([Arc::new(RateLimiter::new(limit))]);
        running.push(control.clone());
        Some(control)
    }

    fn stop(&self) {
        let running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        self.stopped.store(true, Ordering::SeqCst);
        running.iter().for_each(Control::pause);
    }
}

/// How one link of a batch ended.
enum Item {
    Done,
    Failed,
    /// Paused by Ctrl-C, or never started because of it.
    Stopped,
}

/// Download many links, at most `-j` at once, one line per link and a
/// summary. A failed link never stops the others.
async fn run_batch(cli: Cli) -> anyhow::Result<ExitCode> {
    let list = batch_list(&cli)?;
    for s in list.skipped.iter().filter(|s| s.reason.is_range_problem()) {
        eprintln!(
            "Skipped {}: {}",
            redact_urls_in(&s.line),
            s.reason.message(s.count)
        );
    }
    if list.links.is_empty() {
        usage_error("no links to download");
    }
    init_logging(&ProgressBar::hidden(), cli.verbose);
    tracing::info!("rdm {VERSION} · {}", rdm::platform::os_description());

    let dir = std::env::current_dir()?;
    let proxy = cli.proxy.as_deref().map(parse_proxy);
    let claims = TargetClaims::new();
    let stop = Arc::new(BatchStop::default());
    {
        let stop = stop.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                stop.stop();
                if tokio::signal::ctrl_c().await.is_ok() {
                    std::process::exit(INTERRUPTED.into());
                }
            }
        });
    }

    let links: Vec<String> = list.links.into_iter().map(|l| l.url).collect();
    let results = run_limited(links, cli.jobs.into(), |_, url| {
        let (dir, stop, claims) = (dir.clone(), stop.clone(), claims.clone());
        let (connections, proxy, no_proxy) = (cli.connections, proxy.clone(), cli.no_proxy);
        let limit = cli.limit_rate;
        async move {
            let Some(control) = stop.start_item(limit) else {
                return Item::Stopped;
            };
            let mut req = DownloadRequest::new(&url, dir)
                .with_connections(connections)
                .with_claim_target(claims.claim_target());
            if let Some(proxy) = proxy {
                req = req.with_proxy(proxy);
            } else if no_proxy {
                req = req.with_proxy(ProxyMode::None);
            }
            match download(&req, &control, |_| {}).await {
                Ok(summary) => {
                    println!("{}", done_message(&summary, None));
                    Item::Done
                }
                Err(Error::Paused) => Item::Stopped,
                Err(e) => {
                    eprintln!(
                        "Failed {}: {}",
                        redact_url(&url),
                        redact_urls_in(&e.to_string())
                    );
                    Item::Failed
                }
            }
        }
    })
    .await;

    let count = |wanted: fn(&Item) -> bool| results.iter().filter(|r| wanted(r)).count();
    let done = count(|r| matches!(r, Item::Done));
    let failed = count(|r| matches!(r, Item::Failed));
    let stopped = count(|r| matches!(r, Item::Stopped));
    let mut summary = format!("{done} done, {failed} failed");
    if stopped > 0 {
        summary.push_str(&format!(", {stopped} not finished"));
    }
    if !list.skipped.is_empty() {
        summary.push_str(&format!(", {} skipped lines", list.skipped.len()));
    }
    println!("{summary}");

    if stopped > 0 {
        eprintln!("{PAUSED_MESSAGE}");
        return Ok(ExitCode::from(INTERRUPTED));
    }
    if failed > 0 {
        return Ok(ExitCode::FAILURE);
    }
    Ok(power_after(cli.on_finish).await)
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
