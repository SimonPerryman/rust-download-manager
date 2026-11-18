//! Time rules for downloads: a queue window, speed entries and a start-at
//! time, as pure functions of the rules and a local wall-clock moment. Nothing
//! here reads the system time or sleeps; the caller supplies the time (see
//! [`Clock`]).
//!
//! Times are local wall-clock, to the minute. A window follows the wall clock
//! across a clock change, so it is an hour shorter or longer that night.

mod clock;
mod days;
mod scope;
mod time;

pub use clock::{Clock, FakeClock, SystemClock};
pub use days::{Days, DaysName, EmptyDays};
pub use scope::{Scope, Subject};
pub use time::{Date, LocalDateTime, ParseTimeError, TimeOfDay, Weekday};

use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;

/// A window with the same start and end has no length to speak of: it would
/// mean "never" or "always", and the caller means neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("A window can't start and end at the same time")]
pub struct EmptyWindow;

/// A recurring span of the day on chosen days. It may cross midnight
/// (`22:00`–`06:00`), and then belongs to the day it starts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "WindowWire")]
pub struct Window {
    start: TimeOfDay,
    end: TimeOfDay,
    days: Days,
}

#[derive(Deserialize)]
struct WindowWire {
    start: TimeOfDay,
    end: TimeOfDay,
    days: Days,
}

impl TryFrom<WindowWire> for Window {
    type Error = EmptyWindow;

    fn try_from(w: WindowWire) -> Result<Self, EmptyWindow> {
        Self::new(w.start, w.end, w.days)
    }
}

impl Window {
    /// A window from `start` to `end` on `days`; `end` earlier than `start`
    /// runs overnight.
    pub fn new(start: TimeOfDay, end: TimeOfDay, days: Days) -> Result<Self, EmptyWindow> {
        if start == end {
            return Err(EmptyWindow);
        }
        Ok(Self { start, end, days })
    }

    /// When it opens each day it runs.
    pub fn start(&self) -> TimeOfDay {
        self.start
    }

    /// When it closes.
    pub fn end(&self) -> TimeOfDay {
        self.end
    }

    /// The days it opens on.
    pub fn days(&self) -> Days {
        self.days
    }

    fn crosses_midnight(&self) -> bool {
        self.end < self.start
    }

    /// Whether `at` falls inside the window.
    pub fn contains(&self, at: LocalDateTime) -> bool {
        let at = at.effective();
        let (time, day) = (at.time(), at.weekday());
        if self.crosses_midnight() {
            (self.days.contains(day) && time >= self.start)
                || (self.days.contains(day.prev()) && time < self.end)
        } else {
            self.days.contains(day) && time >= self.start && time < self.end
        }
    }

    /// The first moment after `at` the window opens or closes.
    fn next_edge(&self, at: LocalDateTime) -> Option<LocalDateTime> {
        let at = at.effective();
        // Yesterday's start can close today; a week ahead covers every day.
        (-1..=7)
            .map(|offset| at.date().add_days(offset))
            .filter(|date| self.days.contains(date.weekday()))
            .flat_map(|opens| {
                let closes = opens.add_days(i64::from(self.crosses_midnight()));
                [
                    LocalDateTime::new(opens, self.start),
                    LocalDateTime::new(closes, self.end),
                ]
            })
            .filter(|&edge| edge > at)
            .min()
    }
}

/// Downloads start only inside the window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueWindow {
    /// What the user called it; the UI names it from its times when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// A switched-off rule is kept but never applies.
    #[serde(default = "enabled")]
    pub enabled: bool,
    /// When downloads may run.
    pub window: Window,
    /// Let running downloads finish when the window closes, rather than
    /// pausing them.
    #[serde(default)]
    pub finish_running: bool,
    /// The downloads it gates; others ignore it.
    #[serde(default, skip_serializing_if = "Scope::is_all")]
    pub scope: Scope,
}

/// Inside its window, `limit` replaces the global speed limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeedEntry {
    /// What the user called it; the UI names it from its limit when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// A switched-off rule is kept but never applies.
    #[serde(default = "enabled")]
    pub enabled: bool,
    /// When the limit applies.
    pub window: Window,
    /// The cap in bytes/s; never zero, since zero means unlimited elsewhere.
    pub limit: NonZeroU64,
    /// The downloads it limits. For `All` it replaces the global limit; for
    /// anything else it caps the matching downloads together, on top of it.
    #[serde(default, skip_serializing_if = "Scope::is_all")]
    pub scope: Scope,
}

fn enabled() -> bool {
    true
}

/// A one-off time to start a download or batch. There is no end: once started
/// it runs as long as it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StartAt(pub LocalDateTime);

impl StartAt {
    /// Whether the time has come (or passed) at `now`.
    pub fn is_due(&self, now: LocalDateTime) -> bool {
        now.effective() >= self.0
    }

    /// Read a start time typed by a person: `HH:MM` is the next such time
    /// at or after `now` (tonight's, or tomorrow's if it has passed), and
    /// `YYYY-MM-DDTHH:MM` is exactly that moment.
    pub fn parse(text: &str, now: LocalDateTime) -> Result<Self, ParseTimeError> {
        let text = text.trim();
        if text.contains('T') {
            return Ok(Self(text.parse()?));
        }
        let today = LocalDateTime::new(now.date(), text.parse()?);
        Ok(Self(if today >= now.effective() {
            today
        } else {
            today.add_minutes(24 * 60)
        }))
    }
}

/// The rule set, saved with the manager's settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Schedule {
    /// Queue windows; downloads may start while any enabled one is open.
    pub queue_windows: Vec<QueueWindow>,
    /// Speed entries, in priority order.
    pub speed_entries: Vec<SpeedEntry>,
}

impl Schedule {
    /// Whether there are no rules at all.
    pub fn is_empty(&self) -> bool {
        self.queue_windows.is_empty() && self.speed_entries.is_empty()
    }

    fn enabled_windows(&self) -> impl Iterator<Item = &Window> {
        let queue = self.queue_windows.iter().filter(|r| r.enabled);
        let speed = self.speed_entries.iter().filter(|r| r.enabled);
        queue.map(|r| &r.window).chain(speed.map(|r| &r.window))
    }

    /// Whether the queue is open at `at` for downloads in general: always,
    /// unless an enabled all-downloads queue window exists and none is open.
    /// Scoped windows don't count; see [`queue_open_for`](Self::queue_open_for).
    pub fn queue_open(&self, at: LocalDateTime) -> bool {
        self.open_where(at, Scope::is_all)
    }

    /// Whether the queue is open at `at` for one download: always, unless an
    /// enabled window applies to it and none of those is open.
    pub fn queue_open_for(&self, at: LocalDateTime, subject: &Subject<'_>) -> bool {
        self.open_where(at, |scope| scope.matches(subject))
    }

    fn open_where(&self, at: LocalDateTime, applies: impl Fn(&Scope) -> bool) -> bool {
        let mut windows = self
            .queue_windows
            .iter()
            .filter(|r| r.enabled && applies(&r.scope))
            .peekable();
        windows.peek().is_none() || windows.any(|r| r.window.contains(at))
    }

    /// The speed limit in bytes/s that replaces the global one at `at`: the
    /// first enabled all-downloads entry whose window is open, or `None` for
    /// the global limit.
    pub fn speed_limit(&self, at: LocalDateTime) -> Option<u64> {
        self.scoped_speed_limit(at, &Scope::All)
    }

    /// The cap in bytes/s the first enabled entry for exactly `scope` imposes
    /// at `at`, or `None` if none is open. Tags and hosts compare ignoring case.
    pub fn scoped_speed_limit(&self, at: LocalDateTime, scope: &Scope) -> Option<u64> {
        let scope = scope.canonical();
        self.speed_entries
            .iter()
            .find(|r| r.enabled && r.scope.canonical() == scope && r.window.contains(at))
            .map(|r| r.limit.get())
    }

    /// The distinct scopes (other than all downloads) the enabled speed
    /// entries name, in canonical form.
    pub fn speed_scopes(&self) -> Vec<Scope> {
        let mut out: Vec<Scope> = Vec::new();
        for r in self.speed_entries.iter().filter(|r| r.enabled) {
            let scope = r.scope.canonical();
            if !scope.is_all() && !out.contains(&scope) {
                out.push(scope);
            }
        }
        out
    }

    /// The next moment after `at` any enabled rule opens or closes, when the
    /// caller should look again; `None` if no rule will. A moment skipped by a
    /// clock change is still returned, and counts as reached once the wall
    /// clock is at or past it.
    pub fn next_boundary(&self, at: LocalDateTime) -> Option<LocalDateTime> {
        self.enabled_windows().filter_map(|w| w.next_edge(at)).min()
    }

    /// The next moment after `at` the speed limit in force changes (to
    /// another cap, or back to the global one); `None` if it never will.
    pub fn next_speed_change(&self, at: LocalDateTime) -> Option<LocalDateTime> {
        let limit = self.speed_limit(at);
        let mut from = at.effective();
        // As for the queue: overlapping entries can open without changing the cap.
        for _ in 0..(self.speed_entries.len() * 16 + 16) {
            from = self
                .speed_entries
                .iter()
                .filter(|r| r.enabled && r.scope.is_all())
                .filter_map(|r| r.window.next_edge(from))
                .min()?;
            if self.speed_limit(from) != limit {
                return Some(from);
            }
        }
        None
    }

    /// The next moment after `at` the queue flips between open and closed
    /// (what a status line says it is open or paused "until"); `None` if it
    /// never will, as with no queue window or one that is always open.
    pub fn next_queue_change(&self, at: LocalDateTime) -> Option<LocalDateTime> {
        let open = self.queue_open(at);
        let mut from = at.effective();
        // Overlapping windows can open or close without the queue flipping;
        // a week of boundaries per window is far more than any set needs.
        for _ in 0..(self.queue_windows.len() * 16 + 16) {
            from = self
                .queue_windows
                .iter()
                .filter(|r| r.enabled && r.scope.is_all())
                .filter_map(|r| r.window.next_edge(from))
                .min()?;
            if self.queue_open(from) != open {
                return Some(from);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, mo: u8, d: u8, h: u8, mi: u8) -> LocalDateTime {
        LocalDateTime::from_parts(y, mo, d, h, mi).unwrap()
    }

    fn t(s: &str) -> TimeOfDay {
        s.parse().unwrap()
    }

    fn window(start: &str, end: &str, days: Days) -> Window {
        Window::new(t(start), t(end), days).unwrap()
    }

    fn queue(window: Window) -> QueueWindow {
        QueueWindow {
            name: None,
            enabled: true,
            window,
            finish_running: false,
            scope: Scope::All,
        }
    }

    fn speed(window: Window, limit: u64) -> SpeedEntry {
        SpeedEntry {
            name: None,
            enabled: true,
            window,
            limit: NonZeroU64::new(limit).unwrap(),
            scope: Scope::All,
        }
    }

    fn overnight(days: Days) -> Schedule {
        Schedule {
            queue_windows: vec![queue(window("22:00", "06:00", days))],
            ..Schedule::default()
        }
    }

    // 2026-03-09 is a Monday, so 10 is Tue, 11 Wed, 12 Thu.

    #[test]
    fn speed_entries_first_enabled_wins_and_changes_are_found() {
        let weekdays = Days::new([Weekday::Mon, Weekday::Tue]).unwrap();
        let mut off = speed(window("07:00", "19:00", Days::DAILY), 1);
        off.enabled = false;
        let s = Schedule {
            speed_entries: vec![
                off,
                speed(window("08:00", "18:00", weekdays), 5),
                speed(window("12:00", "20:00", Days::DAILY), 9),
            ],
            ..Schedule::default()
        };
        assert_eq!(s.speed_limit(at(2026, 3, 9, 7, 59)), None);
        assert_eq!(s.speed_limit(at(2026, 3, 9, 8, 0)), Some(5));
        assert_eq!(
            s.speed_limit(at(2026, 3, 9, 18, 0)),
            Some(9),
            "falls to the next"
        );
        assert_eq!(s.speed_limit(at(2026, 3, 11, 13, 0)), Some(9));
        // The 12:00 edge of the second entry changes nothing while the first holds.
        assert_eq!(
            s.next_speed_change(at(2026, 3, 9, 8, 0)),
            Some(at(2026, 3, 9, 18, 0))
        );
        assert_eq!(
            s.next_speed_change(at(2026, 3, 9, 20, 0)),
            Some(at(2026, 3, 10, 8, 0))
        );
        assert_eq!(
            Schedule::default().next_speed_change(at(2026, 3, 9, 8, 0)),
            None
        );
    }

    #[test]
    fn weekday_and_date_arithmetic() {
        assert_eq!(at(1970, 1, 1, 0, 0).weekday(), Weekday::Thu);
        assert_eq!(at(2026, 3, 9, 0, 0).weekday(), Weekday::Mon);
        assert_eq!(
            at(2024, 2, 28, 23, 59).add_minutes(2),
            at(2024, 2, 29, 0, 1)
        );
        assert_eq!(
            at(2026, 12, 31, 23, 0).add_minutes(120),
            at(2027, 1, 1, 1, 0)
        );
        assert_eq!(
            at(2026, 3, 1, 0, 30).add_minutes(-60),
            at(2026, 2, 28, 23, 30)
        );
        assert!(Date::new(2026, 2, 29).is_none());
    }

    #[test]
    fn times_parse_and_print() {
        assert_eq!(t("08:05").to_string(), "08:05");
        for bad in ["8:05", "24:00", "12:60", "1200", "12:5", "ab:cd", ""] {
            assert!(bad.parse::<TimeOfDay>().is_err(), "{bad}");
        }
        let when = at(2026, 3, 9, 22, 0);
        assert_eq!(when.to_string().parse(), Ok(when));
        assert!("2026-02-30T10:00".parse::<LocalDateTime>().is_err());
    }

    #[test]
    fn window_crossing_midnight() {
        let s = overnight(Days::DAILY);
        assert!(!s.queue_open(at(2026, 3, 9, 21, 59)));
        assert!(s.queue_open(at(2026, 3, 9, 22, 0)));
        assert!(s.queue_open(at(2026, 3, 10, 0, 0)));
        assert!(s.queue_open(at(2026, 3, 10, 5, 59)));
        assert!(!s.queue_open(at(2026, 3, 10, 6, 0)));
        assert!(!s.queue_open(at(2026, 3, 10, 12, 0)));
    }

    #[test]
    fn window_within_a_day() {
        let s = Schedule {
            queue_windows: vec![queue(window("09:00", "17:00", Days::DAILY))],
            ..Schedule::default()
        };
        assert!(!s.queue_open(at(2026, 3, 9, 8, 59)));
        assert!(s.queue_open(at(2026, 3, 9, 9, 0)));
        assert!(!s.queue_open(at(2026, 3, 9, 17, 0)));
    }

    #[test]
    fn window_on_selected_weekdays() {
        let mon_wed = Days::new([Weekday::Mon, Weekday::Wed]).unwrap();
        let s = overnight(mon_wed);
        assert!(s.queue_open(at(2026, 3, 9, 23, 0))); // Mon night
        assert!(s.queue_open(at(2026, 3, 10, 3, 0))); // Tue small hours belong to Mon
        assert!(!s.queue_open(at(2026, 3, 10, 23, 0))); // Tue night: not chosen
        assert!(!s.queue_open(at(2026, 3, 11, 3, 0))); // Wed small hours belong to Tue
        assert!(s.queue_open(at(2026, 3, 11, 23, 0)));
        assert!(s.queue_open(at(2026, 3, 12, 3, 0)));
        assert!(!s.queue_open(at(2026, 3, 12, 23, 0)));
    }

    #[test]
    fn no_windows_means_always_open_and_a_disabled_one_does_not_count() {
        assert!(Schedule::default().queue_open(at(2026, 3, 9, 12, 0)));
        let mut s = overnight(Days::DAILY);
        assert!(!s.queue_open(at(2026, 3, 9, 12, 0)));
        s.queue_windows[0].enabled = false;
        assert!(s.queue_open(at(2026, 3, 9, 12, 0)));
    }

    #[test]
    fn any_open_window_opens_the_queue() {
        let mut s = overnight(Days::DAILY);
        s.queue_windows
            .push(queue(window("12:00", "13:00", Days::WEEKDAYS)));
        assert!(s.queue_open(at(2026, 3, 9, 12, 30)));
        assert!(!s.queue_open(at(2026, 3, 9, 14, 0)));
    }

    #[test]
    fn speed_entry_inside_and_outside_its_window() {
        let s = Schedule {
            speed_entries: vec![speed(window("08:00", "18:00", Days::WEEKDAYS), 5_000_000)],
            ..Schedule::default()
        };
        assert_eq!(s.speed_limit(at(2026, 3, 9, 7, 59)), None);
        assert_eq!(s.speed_limit(at(2026, 3, 9, 8, 0)), Some(5_000_000));
        assert_eq!(s.speed_limit(at(2026, 3, 9, 17, 59)), Some(5_000_000));
        assert_eq!(s.speed_limit(at(2026, 3, 9, 18, 0)), None);
        assert_eq!(s.speed_limit(at(2026, 3, 14, 12, 0)), None); // Saturday
    }

    #[test]
    fn first_enabled_speed_entry_wins() {
        let mut s = Schedule {
            speed_entries: vec![
                speed(window("08:00", "18:00", Days::DAILY), 1_000),
                speed(window("12:00", "13:00", Days::DAILY), 2_000),
            ],
            ..Schedule::default()
        };
        let noon = at(2026, 3, 9, 12, 30);
        assert_eq!(s.speed_limit(noon), Some(1_000));
        s.speed_entries[0].enabled = false;
        assert_eq!(s.speed_limit(noon), Some(2_000));
    }

    #[test]
    fn next_boundary_across_midnight() {
        let s = overnight(Days::DAILY);
        assert_eq!(
            s.next_boundary(at(2026, 3, 9, 23, 0)),
            Some(at(2026, 3, 10, 6, 0))
        );
        assert_eq!(
            s.next_boundary(at(2026, 3, 10, 6, 0)),
            Some(at(2026, 3, 10, 22, 0))
        );
        assert_eq!(
            s.next_boundary(at(2026, 3, 9, 21, 0)),
            Some(at(2026, 3, 9, 22, 0))
        );
    }

    #[test]
    fn next_boundary_skips_days_the_rule_does_not_run() {
        let s = overnight(Days::new([Weekday::Mon, Weekday::Wed]).unwrap());
        assert_eq!(
            s.next_boundary(at(2026, 3, 9, 23, 0)),
            Some(at(2026, 3, 10, 6, 0))
        );
        assert_eq!(
            s.next_boundary(at(2026, 3, 10, 6, 0)),
            Some(at(2026, 3, 11, 22, 0))
        );
        // From Thursday morning the next opening is the following Monday.
        let s = overnight(Days::new([Weekday::Mon]).unwrap());
        assert_eq!(
            s.next_boundary(at(2026, 3, 12, 6, 0)),
            Some(at(2026, 3, 16, 22, 0))
        );
    }

    #[test]
    fn next_boundary_takes_the_soonest_enabled_rule() {
        let mut s = overnight(Days::DAILY);
        s.speed_entries
            .push(speed(window("08:00", "18:00", Days::DAILY), 1_000));
        assert_eq!(
            s.next_boundary(at(2026, 3, 9, 7, 0)),
            Some(at(2026, 3, 9, 8, 0))
        );
        s.speed_entries[0].enabled = false;
        assert_eq!(
            s.next_boundary(at(2026, 3, 9, 7, 0)),
            Some(at(2026, 3, 9, 22, 0))
        );
        s.queue_windows[0].enabled = false;
        assert_eq!(s.next_boundary(at(2026, 3, 9, 7, 0)), None);
        assert_eq!(
            Schedule::default().next_boundary(at(2026, 3, 9, 7, 0)),
            None
        );
    }

    /// Real minutes until `done` holds, stepping the fake clock one minute at
    /// a time; `change` is a wall-clock jump applied once when the wall clock
    /// first reaches its moment.
    fn real_minutes_until(
        start: LocalDateTime,
        change: (LocalDateTime, i64),
        done: impl Fn(LocalDateTime) -> bool,
    ) -> u32 {
        let clock = FakeClock::new(start);
        let mut changed = false;
        let mut minutes = 0;
        while !done(clock.now()) {
            clock.advance(1);
            minutes += 1;
            if !changed && clock.now() == change.0 {
                changed = true;
                clock.advance(change.1);
            }
            assert!(minutes < 3 * 24 * 60, "never finished");
        }
        minutes
    }

    // US clock changes in 2026: Sunday 8 March, 02:00 -> 03:00 (a 23-hour
    // day) and Sunday 1 November, 02:00 -> 01:00 (a 25-hour day).
    const SPRING: (i32, u8, u8) = (2026, 3, 8);
    const FALL: (i32, u8, u8) = (2026, 11, 1);

    fn night_of(day: (i32, u8, u8), hour: u8) -> LocalDateTime {
        at(day.0, day.1, day.2, hour, 0)
    }

    #[test]
    fn overnight_window_is_an_hour_shorter_when_the_clock_springs_forward() {
        let s = overnight(Days::DAILY);
        let start = night_of(SPRING, 22).add_minutes(-24 * 60);
        let jump = (night_of(SPRING, 2), 60);
        assert_eq!(
            real_minutes_until(start, jump, |now| !s.queue_open(now)),
            7 * 60
        );
        // The same night with no clock change is the usual 8 hours.
        assert_eq!(
            real_minutes_until(start, (at(2000, 1, 1, 0, 0), 0), |now| !s.queue_open(now)),
            8 * 60
        );
    }

    #[test]
    fn overnight_window_is_an_hour_longer_when_the_clock_falls_back() {
        let s = overnight(Days::DAILY);
        let start = night_of(FALL, 22).add_minutes(-24 * 60);
        let back = (night_of(FALL, 2), -60);
        assert_eq!(
            real_minutes_until(start, back, |now| !s.queue_open(now)),
            9 * 60
        );
    }

    #[test]
    fn next_boundary_lands_on_the_wall_clock_on_both_clock_change_days() {
        let s = overnight(Days::DAILY);
        let evening = night_of(SPRING, 23).add_minutes(-24 * 60);
        let closes = s.next_boundary(evening).unwrap();
        assert_eq!(closes, night_of(SPRING, 6));
        // 23:00 to 06:00 is 7 hours, 6 on the 23-hour night.
        let jump = (night_of(SPRING, 2), 60);
        assert_eq!(
            real_minutes_until(evening, jump, |now| now >= closes),
            6 * 60
        );

        let evening = night_of(FALL, 23).add_minutes(-24 * 60);
        let closes = s.next_boundary(evening).unwrap();
        assert_eq!(closes, night_of(FALL, 6));
        let back = (night_of(FALL, 2), -60);
        assert_eq!(
            real_minutes_until(evening, back, |now| now >= closes),
            8 * 60
        );
    }

    #[test]
    fn a_boundary_the_clock_skips_is_reached_once_the_wall_clock_passes_it() {
        // 02:30 doesn't exist on the spring-forward night.
        let s = Schedule {
            queue_windows: vec![queue(window("02:30", "05:00", Days::DAILY))],
            ..Schedule::default()
        };
        let opens = s.next_boundary(night_of(SPRING, 1)).unwrap();
        assert_eq!(opens, at(2026, 3, 8, 2, 30));
        assert!(s.queue_open(at(2026, 3, 8, 3, 0)));
    }

    #[test]
    fn days_name_themselves() {
        let days = |d: &[Weekday]| Days::new(d.iter().copied()).unwrap();
        assert_eq!(Days::new(Weekday::ALL).unwrap().name(), DaysName::Daily);
        assert_eq!(Days::DAILY.to_string(), "Daily");
        let mon_to_fri = days(&Weekday::ALL[..5]);
        assert_eq!(mon_to_fri.name(), DaysName::Weekdays);
        assert_eq!(mon_to_fri.to_string(), "Weekdays");
        let weekend = days(&[Weekday::Sun, Weekday::Sat]);
        assert_eq!(weekend.name(), DaysName::Weekends);
        assert_eq!(weekend.to_string(), "Weekends");

        let some = days(&[Weekday::Thu, Weekday::Mon, Weekday::Tue]);
        assert_eq!(
            some.name(),
            DaysName::Chosen(vec![Weekday::Mon, Weekday::Tue, Weekday::Thu])
        );
        assert_eq!(some.to_string(), "Mon, Tue and Thu");
        assert_eq!(
            days(&[Weekday::Mon, Weekday::Fri]).to_string(),
            "Mon and Fri"
        );
        assert_eq!(days(&[Weekday::Wed]).to_string(), "Wed");
        // Weekdays plus Saturday is no common set.
        assert_eq!(
            days(&[
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
                Weekday::Sat
            ])
            .to_string(),
            "Mon, Tue, Wed, Thu, Fri and Sat"
        );
    }

    #[test]
    fn an_empty_set_of_days_is_refused() {
        assert_eq!(Days::new([]), Err(EmptyDays));
        assert_eq!(EmptyDays.to_string(), "Pick at least one day");
        assert!(serde_json::from_str::<Days>("[]").is_err());
        assert!(
            serde_json::from_str::<Window>(r#"{"start":"22:00","end":"06:00","days":[]}"#).is_err()
        );
    }

    #[test]
    fn a_window_needs_a_length_and_a_limit_needs_a_size() {
        assert_eq!(
            Window::new(t("08:00"), t("08:00"), Days::DAILY),
            Err(EmptyWindow)
        );
        assert!(serde_json::from_str::<Window>(
            r#"{"start":"08:00","end":"08:00","days":["mon"]}"#
        )
        .is_err());
        let zero = r#"{"window":{"start":"08:00","end":"09:00","days":["mon"]},"limit":0}"#;
        assert!(serde_json::from_str::<SpeedEntry>(zero).is_err());
    }

    #[test]
    fn rules_round_trip_through_json() {
        let mut s = overnight(Days::WEEKDAYS);
        s.queue_windows[0].name = Some("Overnight".into());
        s.queue_windows[0].finish_running = true;
        s.speed_entries.push(speed(
            window("08:00", "18:00", Days::new([Weekday::Sat]).unwrap()),
            5_000_000,
        ));
        s.speed_entries[0].enabled = false;
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains(r#""start":"22:00""#), "{json}");
        assert_eq!(serde_json::from_str::<Schedule>(&json).unwrap(), s);
        // Only the window is required; the rest default.
        let min: QueueWindow = serde_json::from_str(
            r#"{"window":{"start":"22:00","end":"06:00","days":["mon","tue"]}}"#,
        )
        .unwrap();
        assert!(min.enabled && !min.finish_running && min.name.is_none());
    }

    #[test]
    fn start_at_is_due_from_its_minute() {
        let start = StartAt(at(2026, 3, 9, 22, 0));
        assert!(!start.is_due(at(2026, 3, 9, 21, 59)));
        assert!(start.is_due(at(2026, 3, 9, 22, 0)));
        assert!(start.is_due(at(2026, 3, 10, 0, 0)));
        let json = serde_json::to_string(&start).unwrap();
        assert_eq!(json, r#""2026-03-09T22:00""#);
        assert_eq!(serde_json::from_str::<StartAt>(&json).unwrap(), start);
    }

    #[test]
    fn a_typed_start_time_means_the_next_one() {
        let now = at(2026, 3, 9, 21, 0);
        let parse = |s| StartAt::parse(s, now).map(|s| s.0);
        assert_eq!(parse("22:00"), Ok(at(2026, 3, 9, 22, 0)));
        assert_eq!(parse("21:00"), Ok(at(2026, 3, 9, 21, 0)), "this minute");
        assert_eq!(
            parse("01:30"),
            Ok(at(2026, 3, 10, 1, 30)),
            "passed: tomorrow"
        );
        assert_eq!(parse(" 2026-04-01T08:05 "), Ok(at(2026, 4, 1, 8, 5)));
        for bad in ["", "25:00", "9:00", "noon", "2026-13-01T00:00"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn fake_clock_moves_only_when_told() {
        let clock = FakeClock::new(at(2026, 3, 9, 23, 59));
        assert_eq!(clock.now(), at(2026, 3, 9, 23, 59));
        clock.advance(1);
        assert_eq!(clock.now(), at(2026, 3, 10, 0, 0));
        clock.set(at(2026, 1, 1, 0, 0));
        assert_eq!(clock.now(), at(2026, 1, 1, 0, 0));
    }

    #[test]
    fn a_boundary_in_the_repeated_hour_is_crossed_once() {
        // Closes at 01:30; the clock falls back from 02:00 to 01:00.
        let s = Schedule {
            queue_windows: vec![queue(window("22:00", "01:30", Days::DAILY))],
            ..Schedule::default()
        };
        let clock = FakeClock::new(night_of(FALL, 1).add_minutes(29));
        assert!(s.queue_open(clock.now()));
        clock.advance(1);
        assert!(!s.queue_open(clock.now()), "closed at 01:30");
        clock.advance(30);
        clock.fall_back(60);
        let again = clock.now();
        assert_eq!(again.time(), t("01:00"));
        assert!(again.repeat_end().is_some());
        // By the bare wall clock this is inside the window again.
        assert!(s.queue_open(LocalDateTime::new(again.date(), again.time())));
        assert!(!s.queue_open(again), "the repeat doesn't reopen it");
        clock.advance(45);
        assert!(!s.queue_open(clock.now()), "01:45, second pass");
        assert_eq!(
            s.next_boundary(clock.now()),
            Some(night_of(FALL, 22)),
            "nothing left in the repeat to wait for"
        );
        clock.advance(15);
        assert_eq!(clock.now(), night_of(FALL, 2), "the repeat has ended");
        assert!(!s.queue_open(clock.now()));
    }

    #[test]
    fn a_boundary_the_repeat_opens_on_is_not_met_twice_either() {
        // Opens at 01:30, inside the repeated hour: open from the first
        // pass, and still open (not re-triggered) in the second.
        let s = Schedule {
            queue_windows: vec![queue(window("01:30", "05:00", Days::DAILY))],
            ..Schedule::default()
        };
        let clock = FakeClock::new(night_of(FALL, 1).add_minutes(30));
        assert!(s.queue_open(clock.now()));
        clock.advance(30);
        clock.fall_back(60);
        assert!(s.queue_open(clock.now()), "01:00 again, still open");
    }

    #[test]
    fn a_start_at_inside_the_repeat_is_due_in_the_second_pass() {
        let start = StartAt(night_of(FALL, 1).add_minutes(30));
        let clock = FakeClock::new(night_of(FALL, 2));
        clock.fall_back(60);
        assert!(start.is_due(clock.now()));
    }

    #[test]
    fn the_next_queue_change_is_when_the_queue_flips() {
        let mut s = overnight(Days::DAILY);
        s.queue_windows
            .push(queue(window("23:00", "07:00", Days::DAILY)));
        // Overlapping windows: the queue stays open through 22:00-07:00.
        assert_eq!(
            s.next_queue_change(at(2026, 3, 9, 12, 0)),
            Some(at(2026, 3, 9, 22, 0))
        );
        assert_eq!(
            s.next_queue_change(at(2026, 3, 9, 23, 30)),
            Some(at(2026, 3, 10, 7, 0))
        );
        assert_eq!(
            Schedule::default().next_queue_change(at(2026, 3, 9, 12, 0)),
            None
        );
    }

    #[test]
    fn the_system_clock_reads_a_plausible_unrepeated_minute() {
        let now = SystemClock::new().now();
        assert!(now.date() > Date::new(2025, 1, 1).unwrap());
        assert!(now.repeat_end().is_none());
    }

    #[test]
    fn rules_saved_before_scopes_load_as_all_downloads_and_save_unchanged() {
        let json = serde_json::to_string(&Schedule {
            queue_windows: vec![queue(window("22:00", "06:00", Days::DAILY))],
            ..Schedule::default()
        })
        .unwrap();
        assert!(
            !json.contains("scope"),
            "an all-downloads rule writes no scope"
        );
        let loaded: Schedule = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.queue_windows[0].scope, Scope::All);
    }

    #[test]
    fn a_scoped_rule_round_trips_and_only_binds_its_downloads() {
        let mut rule = queue(window("22:00", "06:00", Days::DAILY));
        rule.scope = Scope::Tag { tag: "x".into() };
        let schedule = Schedule {
            queue_windows: vec![rule],
            ..Schedule::default()
        };
        let back: Schedule =
            serde_json::from_str(&serde_json::to_string(&schedule).unwrap()).unwrap();
        assert_eq!(back, schedule);
        let noon = at(2026, 3, 9, 12, 0);
        assert!(
            schedule.queue_open(noon),
            "a scoped window isn't a general gate"
        );
        let tags = vec!["X".to_string()];
        let mine = Subject {
            tags: &tags,
            url: "http://a/b",
            batch: None,
        };
        let other = Subject {
            tags: &[],
            url: "http://a/b",
            batch: None,
        };
        assert!(!schedule.queue_open_for(noon, &mine));
        assert!(schedule.queue_open_for(noon, &other));
        assert_eq!(schedule.next_queue_change(noon), None);
    }
}
