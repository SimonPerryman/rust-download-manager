//! The ramp's own account of a download, its `ramp window` trace lines, so a
//! test waits on what the ramp decided rather than on time passing: a loaded
//! runner stretches and skews the windows, but each still says what it saw.

use std::time::{Duration, Instant};

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Layer;

use super::{Captured, Lines};

/// How long a wait goes without a single new ramp window before it fails:
/// the download stopped measuring, so no window budget would ever run out.
const SILENT: Duration = Duration::from_secs(30);

/// One `ramp window` trace line: what the ramp measured and decided.
#[derive(Debug, Clone)]
pub struct RampWindow {
    pub decision: String,
    /// The connections the ramp wants after this window.
    pub target: u8,
    /// The speed the window measured, in bytes/s.
    pub bytes_per_sec: f64,
    /// Neither skipped (a pool change, a warming lane, the work running out,
    /// a stall) nor slowed by a speed limit.
    pub judged: bool,
}

impl RampWindow {
    /// Judged and kept at `n`: only a settled ramp holds on a judged window
    /// (a ramping one adds or retires), so the ramp is settled at `n`.
    pub fn settled_at(&self, n: u8) -> bool {
        self.judged && self.target == n && self.decision == "Hold"
    }

    fn parse(line: &str) -> Self {
        let field = |name: &str| -> String {
            let at = line.find(name).expect("traced field") + name.len();
            line[at..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
                .collect()
        };
        let bytes_per_sec: f64 = field("bytes_per_sec: ").parse().expect("numeric speed");
        Self {
            decision: field(" decision="),
            target: field(" target=").parse().expect("numeric target"),
            bytes_per_sec,
            judged: [
                "waited_fraction: 0.0,",
                "pool_changed: false",
                "short: false",
                "warming: false",
            ]
            .iter()
            .all(|f| line.contains(f))
                && bytes_per_sec > 0.0,
        }
    }
}

/// The ramp's `ramp window` trace lines, in order, from the first line
/// containing `from` on.
pub fn ramp_windows(log: &[String], from: &str) -> Vec<RampWindow> {
    let start = log
        .iter()
        .position(|l| l.contains(from))
        .unwrap_or_else(|| panic!("no {from:?} in the log"));
    windows(&log[start..])
}

fn windows(log: &[String]) -> Vec<RampWindow> {
    log.iter()
        .filter(|l| l.contains("ramp window"))
        .map(|l| RampWindow::parse(l))
        .collect()
}

/// A running download's ramp windows, read as they're traced.
pub struct RampTrace(Captured);

impl RampTrace {
    /// Trace this thread's ramp windows until the guard drops.
    pub fn capture() -> (Self, tracing::subscriber::DefaultGuard) {
        let (log, guard) = super::capture_log("rdm=trace");
        (Self(log), guard)
    }

    /// [`RampTrace::capture`], and the concise and detail lines into `lines`
    /// as [`capture`](super::capture) does.
    pub fn capture_with(lines: &Lines) -> (Self, tracing::subscriber::DefaultGuard) {
        let log = Captured::default();
        let trace = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(log.clone())
            .with_filter(
                tracing_subscriber::filter::Targets::new()
                    .with_target("rdm", tracing::Level::TRACE),
            );
        let subscriber = tracing_subscriber::registry()
            .with(lines.clone())
            .with(trace);
        (Self(log), tracing::subscriber::set_default(subscriber))
    }

    /// Every window so far.
    pub fn windows(&self) -> Vec<RampWindow> {
        windows(&self.0.lines())
    }

    /// Wait until `done` holds for the windows so far, and return how many
    /// there were. Fails once `within` more windows pass without it.
    pub async fn until(
        &self,
        what: &str,
        within: usize,
        mut done: impl FnMut(&[RampWindow]) -> bool,
    ) -> usize {
        let start = self.windows().len();
        let (mut seen, mut since) = (start, Instant::now());
        loop {
            let windows = self.windows();
            if done(&windows) {
                return windows.len();
            }
            assert!(
                windows.len() < start + within,
                "never {what} within {within} ramp windows"
            );
            if windows.len() > seen {
                (seen, since) = (windows.len(), Instant::now());
            }
            assert!(
                since.elapsed() < SILENT,
                "no ramp window for {SILENT:?}, and never {what}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Wait for the first window from the `from`th on that `matches`, and
    /// return its index. Fails once `within` more windows pass without one.
    pub async fn first(
        &self,
        what: &str,
        within: usize,
        from: usize,
        matches: impl Fn(&RampWindow) -> bool,
    ) -> usize {
        let mut at = None;
        self.until(what, within, |windows| {
            at = windows
                .iter()
                .skip(from)
                .position(&matches)
                .map(|i| i + from);
            at.is_some()
        })
        .await;
        at.expect("found")
    }
}
