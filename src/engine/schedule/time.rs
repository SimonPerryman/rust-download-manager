//! Local wall-clock time as plain data: a date, a minute of the day and a
//! weekday. No zone database, so rules are pure functions of these values.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

const MINUTES_PER_DAY: i64 = 24 * 60;

/// A day of the week, Monday first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    /// All seven days, Monday first.
    pub const ALL: [Weekday; 7] = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
        Weekday::Sat,
        Weekday::Sun,
    ];

    pub(crate) fn index(self) -> u8 {
        self as u8
    }

    /// The day before this one (Monday's is Sunday).
    pub fn prev(self) -> Weekday {
        Self::ALL[(self as usize + 6) % 7]
    }

    /// The three-letter name, as the UI shows it.
    pub fn short(self) -> &'static str {
        match self {
            Weekday::Mon => "Mon",
            Weekday::Tue => "Tue",
            Weekday::Wed => "Wed",
            Weekday::Thu => "Thu",
            Weekday::Fri => "Fri",
            Weekday::Sat => "Sat",
            Weekday::Sun => "Sun",
        }
    }
}

/// Why a time or date string was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ParseTimeError(String);

/// A clock time within one day, to the minute (`22:00`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TimeOfDay(u16);

impl TimeOfDay {
    /// `hour` 0..=23 and `minute` 0..=59, else `None`.
    pub fn new(hour: u8, minute: u8) -> Option<Self> {
        (hour < 24 && minute < 60).then(|| Self(u16::from(hour) * 60 + u16::from(minute)))
    }
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.0 / 60, self.0 % 60)
    }
}

impl FromStr for TimeOfDay {
    type Err = ParseTimeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || ParseTimeError(format!("'{s}' isn't a time; use HH:MM, like 22:00"));
        let (h, m) = s.split_once(':').ok_or_else(bad)?;
        if h.len() != 2 || m.len() != 2 {
            return Err(bad());
        }
        let h: u8 = h.parse().map_err(|_| bad())?;
        let m: u8 = m.parse().map_err(|_| bad())?;
        Self::new(h, m).ok_or_else(bad)
    }
}

impl Serialize for TimeOfDay {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for TimeOfDay {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// A calendar date (proleptic Gregorian).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Date {
    year: i32,
    month: u8,
    day: u8,
}

impl Date {
    /// A real calendar date, else `None`.
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let last = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return None,
        };
        (1..=last)
            .contains(&day)
            .then_some(Self { year, month, day })
    }

    /// The weekday this date falls on.
    pub fn weekday(self) -> Weekday {
        // Day 0 (1970-01-01) was a Thursday, which is index 3.
        Weekday::ALL[(self.days_since_epoch() + 3).rem_euclid(7) as usize]
    }

    /// The date `days` later (earlier if negative).
    pub fn add_days(self, days: i64) -> Self {
        Self::from_days_since_epoch(self.days_since_epoch() + days)
    }

    // Howard Hinnant's civil-date algorithms.
    fn days_since_epoch(self) -> i64 {
        let (y, m, d) = (
            i64::from(self.year),
            i64::from(self.month),
            i64::from(self.day),
        );
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    fn from_days_since_epoch(days: i64) -> Self {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        Self {
            year: y as i32,
            month: m as u8,
            day: d as u8,
        }
    }
}

/// A wall-clock moment in the machine's local zone, to the minute. It carries
/// no offset: across a clock change the wall clock skips or repeats an hour,
/// and rules follow the wall clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocalDateTime {
    date: Date,
    time: TimeOfDay,
}

impl LocalDateTime {
    /// Combine a date and a time of day.
    pub fn new(date: Date, time: TimeOfDay) -> Self {
        Self { date, time }
    }

    /// A literal moment, `None` if it isn't a real one.
    pub fn from_parts(year: i32, month: u8, day: u8, hour: u8, minute: u8) -> Option<Self> {
        Some(Self::new(
            Date::new(year, month, day)?,
            TimeOfDay::new(hour, minute)?,
        ))
    }

    /// The calendar date.
    pub fn date(self) -> Date {
        self.date
    }

    /// The time of day.
    pub fn time(self) -> TimeOfDay {
        self.time
    }

    /// The weekday of the date.
    pub fn weekday(self) -> Weekday {
        self.date.weekday()
    }

    /// The moment `minutes` later on the wall clock (earlier if negative).
    pub fn add_minutes(self, minutes: i64) -> Self {
        let total = i64::from(self.time.0) + minutes;
        let date = self.date.add_days(total.div_euclid(MINUTES_PER_DAY));
        let time = TimeOfDay(total.rem_euclid(MINUTES_PER_DAY) as u16);
        Self { date, time }
    }
}

impl fmt::Display for LocalDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Date { year, month, day } = self.date;
        write!(f, "{year:04}-{month:02}-{day:02}T{}", self.time)
    }
}

impl FromStr for LocalDateTime {
    type Err = ParseTimeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || ParseTimeError(format!("'{s}' isn't a date and time; use YYYY-MM-DDTHH:MM"));
        let (date, time) = s.split_once('T').ok_or_else(bad)?;
        let mut parts = date.split('-');
        let (Some(y), Some(m), Some(d), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(bad());
        };
        let date = Date::new(
            y.parse().map_err(|_| bad())?,
            m.parse().map_err(|_| bad())?,
            d.parse().map_err(|_| bad())?,
        )
        .ok_or_else(bad)?;
        Ok(Self::new(date, time.parse()?))
    }
}

impl Serialize for LocalDateTime {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for LocalDateTime {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
