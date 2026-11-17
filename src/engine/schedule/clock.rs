//! The injected clock: the only way the schedule learns the time.

use super::time::LocalDateTime;
use crate::engine::lock::lock;
use std::sync::Mutex;

/// A source of the machine's local wall-clock time. Injected so tests run on a
/// fake and never wait for real.
pub trait Clock: Send + Sync {
    /// The current local time, to the minute. In the second pass through an
    /// hour the clock repeated, it is marked as such (see
    /// [`LocalDateTime::repeating_until`]).
    fn now(&self) -> LocalDateTime;
}

/// The machine's clock, in its local time zone.
#[derive(Debug, Default)]
pub struct SystemClock(Mutex<Seen>);

/// What the last reading saw, to notice the clock being set back.
#[derive(Debug, Default)]
struct Seen {
    last: Option<(i32, LocalDateTime)>,
    repeat_end: Option<LocalDateTime>,
}

impl SystemClock {
    /// The system clock.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> LocalDateTime {
        let (offset, local) = sample();
        let mut seen = lock(&self.0);
        if let Some((before, last)) = seen.last {
            // The UTC offset dropped (a fall-back): the wall hour just lived
            // repeats, and it ends where the last reading left off.
            if offset < before {
                seen.repeat_end = Some(last.add_minutes(1));
            }
        }
        seen.last = Some((offset, local));
        match seen.repeat_end {
            Some(end) if local < end => local.repeating_until(end),
            _ => {
                seen.repeat_end = None;
                local
            }
        }
    }
}

/// The local wall-clock minute and the UTC offset in minutes (east positive).
fn sample() -> (i32, LocalDateTime) {
    let (local, utc) = os::local_and_utc();
    // A reading taken either side of a minute boundary would show a
    // one-minute offset; real offsets are quarter-hours.
    let offset = (local.minutes_since(utc) as f64 / 15.0).round() as i32 * 15;
    (offset, local)
}

#[cfg(windows)]
mod os {
    use super::LocalDateTime;
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};

    fn minute(t: &SYSTEMTIME) -> LocalDateTime {
        LocalDateTime::from_parts(
            i32::from(t.wYear),
            t.wMonth as u8,
            t.wDay as u8,
            t.wHour as u8,
            t.wMinute as u8,
        )
        .expect("the OS reports a real date")
    }

    pub(super) fn local_and_utc() -> (LocalDateTime, LocalDateTime) {
        // SAFETY: both calls only fill the struct they are handed.
        let (local, utc) = unsafe {
            let (mut local, mut utc): (SYSTEMTIME, SYSTEMTIME) = std::mem::zeroed();
            GetLocalTime(&mut local);
            GetSystemTime(&mut utc);
            (local, utc)
        };
        (minute(&local), minute(&utc))
    }
}

#[cfg(unix)]
mod os {
    use super::LocalDateTime;
    use std::time::{SystemTime, UNIX_EPOCH};

    pub(super) fn local_and_utc() -> (LocalDateTime, LocalDateTime) {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as libc::time_t);
        // SAFETY: `localtime_r` and `gmtime_r` only fill the `tm` they are given.
        let (local, utc) = unsafe {
            let (mut local, mut utc): (libc::tm, libc::tm) = std::mem::zeroed();
            libc::localtime_r(&secs, &mut local);
            libc::gmtime_r(&secs, &mut utc);
            (local, utc)
        };
        let minute = |t: &libc::tm| {
            LocalDateTime::from_parts(
                t.tm_year + 1900,
                (t.tm_mon + 1) as u8,
                t.tm_mday as u8,
                t.tm_hour as u8,
                t.tm_min as u8,
            )
            .expect("the OS reports a real date")
        };
        (minute(&local), minute(&utc))
    }
}

/// A clock tests move by hand.
#[derive(Debug)]
pub struct FakeClock(Mutex<LocalDateTime>);

impl FakeClock {
    /// A clock standing at `at`.
    pub fn new(at: LocalDateTime) -> Self {
        Self(Mutex::new(at))
    }

    /// Jump to `at`.
    pub fn set(&self, at: LocalDateTime) {
        *lock(&self.0) = at;
    }

    /// Move the wall clock by `minutes`. A real minute is `advance(1)`; a
    /// clock change is an extra `advance(60)` (spring forward) or
    /// [`fall_back`](Self::fall_back) (which marks the repeated hour).
    pub fn advance(&self, minutes: i64) {
        let mut now = lock(&self.0);
        let next = now.add_minutes(minutes);
        *now = match now.repeat_end() {
            Some(end) if next < end => next.repeating_until(end),
            _ => next,
        };
    }

    /// Set the wall clock back by `minutes`, as at the end of daylight
    /// saving: the hours just lived repeat, marked until the wall clock
    /// reaches where it was set back from.
    pub fn fall_back(&self, minutes: i64) {
        let mut now = lock(&self.0);
        *now = now.add_minutes(-minutes).repeating_until(*now);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> LocalDateTime {
        *lock(&self.0)
    }
}
