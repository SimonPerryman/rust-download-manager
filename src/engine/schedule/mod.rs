//! Time rules for downloads: a queue window, speed entries and a start-at
//! time, as pure functions of the rules and a local wall-clock moment. Nothing
//! here reads the system time or sleeps; the caller supplies the time (see
//! [`Clock`]).
//!
//! Times are local wall-clock, to the minute. A window follows the wall clock
//! across a clock change, so it is an hour shorter or longer that night.

mod clock;
mod days;
mod time;

pub use clock::{Clock, FakeClock};
pub use days::{Days, DaysName, EmptyDays};
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
        now >= self.0
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

    /// Whether the queue is open at `at`: always, unless an enabled queue
    /// window exists and none is open.
    pub fn queue_open(&self, at: LocalDateTime) -> bool {
        let mut windows = self.queue_windows.iter().filter(|r| r.enabled).peekable();
        windows.peek().is_none() || windows.any(|r| r.window.contains(at))
    }

    /// The speed limit in bytes/s that replaces the global one at `at`: the
    /// first enabled entry whose window is open, or `None` for the global
    /// limit.
    pub fn speed_limit(&self, at: LocalDateTime) -> Option<u64> {
        self.speed_entries
            .iter()
            .find(|r| r.enabled && r.window.contains(at))
            .map(|r| r.limit.get())
    }

    /// The next moment after `at` any enabled rule opens or closes, when the
    /// caller should look again; `None` if no rule will. A moment skipped by a
    /// clock change is still returned, and counts as reached once the wall
    /// clock is at or past it.
    pub fn next_boundary(&self, at: LocalDateTime) -> Option<LocalDateTime> {
        self.enabled_windows().filter_map(|w| w.next_edge(at)).min()
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
        }
    }

    fn speed(window: Window, limit: u64) -> SpeedEntry {
        SpeedEntry {
            name: None,
            enabled: true,
            window,
            limit: NonZeroU64::new(limit).unwrap(),
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
    fn fake_clock_moves_only_when_told() {
        let clock = FakeClock::new(at(2026, 3, 9, 23, 59));
        assert_eq!(clock.now(), at(2026, 3, 9, 23, 59));
        clock.advance(1);
        assert_eq!(clock.now(), at(2026, 3, 10, 0, 0));
        clock.set(at(2026, 1, 1, 0, 0));
        assert_eq!(clock.now(), at(2026, 1, 1, 0, 0));
    }
}
