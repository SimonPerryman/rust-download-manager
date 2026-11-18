//! The days of the week a rule runs on.

use super::time::Weekday;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// A set of days was empty; a rule on no days could never run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("Pick at least one day")]
pub struct EmptyDays;

/// A non-empty set of weekdays. "Daily", "Weekdays" and "Weekends" are only
/// names for common sets (see [`Days::name`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Days(u8);

/// What a set of days is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaysName {
    /// All seven days.
    Daily,
    /// Monday to Friday.
    Weekdays,
    /// Saturday and Sunday.
    Weekends,
    /// Any other set, Monday first.
    Chosen(Vec<Weekday>),
}

impl Days {
    /// Every day.
    pub const DAILY: Days = Days(0b111_1111);
    /// Monday to Friday.
    pub const WEEKDAYS: Days = Days(0b001_1111);
    /// Saturday and Sunday.
    pub const WEEKENDS: Days = Days(0b110_0000);

    /// The given days. An empty set is refused here, so the GUI and CLI share
    /// the rule.
    pub fn new(days: impl IntoIterator<Item = Weekday>) -> Result<Self, EmptyDays> {
        let bits = days.into_iter().fold(0u8, |b, d| b | 1 << d.index());
        if bits == 0 {
            Err(EmptyDays)
        } else {
            Ok(Self(bits))
        }
    }

    /// Whether `day` is in the set.
    pub fn contains(self, day: Weekday) -> bool {
        self.0 & (1 << day.index()) != 0
    }

    /// The days in the set, Monday first.
    pub fn iter(self) -> impl Iterator<Item = Weekday> {
        Weekday::ALL.into_iter().filter(move |&d| self.contains(d))
    }

    /// The set's name, derived from its contents and never stored.
    pub fn name(self) -> DaysName {
        match self {
            Self::DAILY => DaysName::Daily,
            Self::WEEKDAYS => DaysName::Weekdays,
            Self::WEEKENDS => DaysName::Weekends,
            _ => DaysName::Chosen(self.iter().collect()),
        }
    }
}

impl fmt::Display for Days {
    /// `Daily`, `Weekdays`, `Weekends`, or the days: `Mon, Tue and Thu`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            DaysName::Daily => f.write_str("Daily"),
            DaysName::Weekdays => f.write_str("Weekdays"),
            DaysName::Weekends => f.write_str("Weekends"),
            DaysName::Chosen(days) => {
                let (last, rest) = days.split_last().expect("a Days set is never empty");
                for (i, d) in rest.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    f.write_str(d.short())?;
                }
                if !rest.is_empty() {
                    f.write_str(" and ")?;
                }
                f.write_str(last.short())
            }
        }
    }
}

impl Serialize for Days {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.iter())
    }
}

impl<'de> Deserialize<'de> for Days {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(Vec::<Weekday>::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}
