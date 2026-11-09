//! Sizes, speeds and durations as people read them, for log lines. Sizes
//! match the GUI's `formatBytes` (1024-based, `KB`/`MB`/`GB` labels).

use std::time::Duration;

/// A byte count, e.g. `512 B`, `8.60 MB`, `11.4 GB`.
pub(crate) fn bytes(n: u64) -> String {
    if n < 1024 {
        return format!("{n} B");
    }
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    let mut value = n as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{value:.2} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// A transfer rate, e.g. `14.4 MB/s`.
pub(crate) fn speed(bytes_per_sec: f64) -> String {
    format!("{}/s", bytes(bytes_per_sec.max(0.0).round() as u64))
}

/// A duration, e.g. `0.4s`, `42s`, `9m 59s`, `1h 5m`.
pub(crate) fn duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 10 {
        return format!("{:.1}s", d.as_secs_f64());
    }
    if secs < 60 {
        return format!("{secs}s");
    }
    if secs < 3600 {
        return format!("{}m {}s", secs / 60, secs % 60);
    }
    format!("{}h {}m", secs / 3600, secs % 3600 / 60)
}

/// `n` of a countable thing, e.g. `1 retry`, `2 retries`.
pub(crate) fn count(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_the_gui() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1024), "1.00 KB");
        assert_eq!(bytes(4 * 1024 * 1024), "4.00 MB");
        assert_eq!(bytes(6_710_886_400), "6.25 GB");
        assert_eq!(bytes(15 * 1024 * 1024 + 1024 * 1024 / 2), "15.5 MB");
    }

    #[test]
    fn speeds_are_sizes_per_second() {
        assert_eq!(speed(15_099_494.4), "14.4 MB/s");
        assert_eq!(speed(-1.0), "0 B/s");
    }

    #[test]
    fn durations_stay_short() {
        assert_eq!(duration(Duration::from_millis(420)), "0.4s");
        assert_eq!(duration(Duration::from_secs(42)), "42s");
        assert_eq!(duration(Duration::from_secs(599)), "9m 59s");
        assert_eq!(duration(Duration::from_secs(3900)), "1h 5m");
    }

    #[test]
    fn counts_pick_the_right_word() {
        assert_eq!(count(1, "retry", "retries"), "1 retry");
        assert_eq!(count(2, "retry", "retries"), "2 retries");
    }
}
