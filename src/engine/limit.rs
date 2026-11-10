//! Byte-rate limiting: a token bucket shared by every connection it governs,
//! and the rate grammar (`500K`, `2M`) shared by the CLI and GUI.

use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

use crate::engine::lock::lock;

/// Smallest bucket capacity, so a low rate still lets a whole chunk through.
const MIN_BURST: u64 = 64 * 1024;

/// A token bucket that may go into debt: a chunk that has already arrived is
/// always accepted, and its excess becomes a wait before the next one.
///
/// Shared (`Arc`) between every connection it limits; [`set_rate`](Self::set_rate)
/// applies live to downloads already throttled by it.
pub struct RateLimiter {
    state: Mutex<Bucket>,
}

struct Bucket {
    rate: Option<u64>,
    /// May be negative (debt).
    tokens: f64,
    last: Instant,
}

impl Bucket {
    /// Credit the tokens earned since `last` at the current rate, capped at
    /// the burst.
    fn refill(&mut self, now: Instant) {
        if let Some(rate) = self.rate {
            let earned = now.duration_since(self.last).as_secs_f64() * rate as f64;
            self.tokens = (self.tokens + earned).min(burst_for(rate) as f64);
        }
        self.last = now;
    }
}

fn burst_for(rate: u64) -> u64 {
    (rate / 4).max(MIN_BURST)
}

fn normalise(rate: Option<u64>) -> Option<u64> {
    rate.filter(|&r| r > 0)
}

impl RateLimiter {
    /// A limiter capped at `rate` bytes/s, starting with a full burst;
    /// `None` (or `Some(0)`) is unlimited.
    pub fn new(rate: Option<u64>) -> Self {
        let rate = normalise(rate);
        Self {
            state: Mutex::new(Bucket {
                rate,
                tokens: rate.map_or(0.0, |r| burst_for(r) as f64),
                last: Instant::now(),
            }),
        }
    }

    /// The current cap in bytes/s, `None` when unlimited.
    pub fn rate(&self) -> Option<u64> {
        lock(&self.state).rate
    }

    /// Bytes that may pass without waiting after an idle spell: a quarter
    /// second of the rate, at least 64 KiB (the minimum while unlimited).
    pub fn burst(&self) -> u64 {
        self.rate().map_or(MIN_BURST, burst_for)
    }

    /// Change the cap live (`None`/`Some(0)` = unlimited). Debt already owed
    /// is kept, so changing the rate can't be used to skip a wait.
    pub fn set_rate(&self, rate: Option<u64>) {
        let rate = normalise(rate);
        let mut bucket = lock(&self.state);
        // Settle what was earned at the old rate before the new one applies.
        bucket.refill(Instant::now());
        bucket.tokens = match (bucket.rate, rate) {
            (_, None) => bucket.tokens,
            // Traffic while unlimited never accrued debt; start afresh.
            (None, Some(new)) => burst_for(new) as f64,
            (Some(_), Some(new)) => bucket.tokens.min(burst_for(new) as f64),
        };
        bucket.rate = rate;
    }

    /// Debit `n` bytes and return when the balance is back to zero, or `None`
    /// if there is nothing to wait for. `charge(0)` is a non-debiting peek.
    /// Never blocks.
    pub(crate) fn charge(&self, n: u64) -> Option<Instant> {
        let mut bucket = lock(&self.state);
        let rate = bucket.rate?;
        let now = Instant::now();
        bucket.refill(now);
        bucket.tokens -= n as f64;
        if bucket.tokens >= 0.0 {
            return None;
        }
        let wait = Duration::from_secs_f64(-bucket.tokens / rate as f64);
        (!wait.is_zero()).then(|| now + wait)
    }

    #[cfg(test)]
    pub(crate) fn balance(&self) -> f64 {
        lock(&self.state).tokens
    }
}

/// Parse a byte rate: a number of bytes/s with an optional binary suffix
/// (`K`, `M`, `G`, ×1024, case-insensitive), e.g. `500K`, `1.5M`, `2048`.
/// Shared by every frontend so they accept one grammar.
pub fn parse_rate(s: &str) -> Result<u64, String> {
    let invalid = || format!("invalid rate {s:?}: expected bytes/s like 500K, 2M or 1.5G");
    let (number, multiplier) = match s.char_indices().last() {
        Some((i, c)) if !c.is_ascii_digit() => {
            let multiplier: u64 = match c.to_ascii_uppercase() {
                'K' => 1 << 10,
                'M' => 1 << 20,
                'G' => 1 << 30,
                _ => return Err(invalid()),
            };
            (&s[..i], multiplier)
        }
        _ => (s, 1),
    };
    let (whole, fraction) = match number.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (number, None),
    };
    let all_digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(whole) || !fraction.is_none_or(all_digits) {
        return Err(invalid());
    }

    let too_large = || format!("rate {s:?} is too large");
    let whole: u64 = whole.parse().map_err(|_| too_large())?;
    let fraction_bytes = fraction.map_or(0, |f| {
        let f: f64 = format!("0.{f}").parse().unwrap_or(0.0);
        (f * multiplier as f64) as u64
    });
    let rate = whole
        .checked_mul(multiplier)
        .and_then(|bytes| bytes.checked_add(fraction_bytes))
        .ok_or_else(too_large)?;
    if rate == 0 {
        return Err(format!("rate {s:?} must be greater than zero"));
    }
    Ok(rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;

    fn secs(d: Duration) -> f64 {
        d.as_secs_f64()
    }

    #[test]
    fn burst_is_a_quarter_second_but_at_least_64k() {
        assert_eq!(RateLimiter::new(Some(4 * MIB)).burst(), MIB);
        assert_eq!(RateLimiter::new(Some(100 * KIB)).burst(), 64 * KIB);
        assert_eq!(RateLimiter::new(None).burst(), 64 * KIB);
    }

    #[tokio::test(start_paused = true)]
    async fn debt_deadline_is_the_time_to_repay_it() {
        let limiter = RateLimiter::new(Some(MIB));
        let burst = limiter.burst();
        let start = Instant::now();
        assert_eq!(limiter.charge(burst), None, "a full burst passes freely");
        let deadline = limiter.charge(MIB / 2).expect("in debt");
        assert!((secs(deadline - start) - 0.5).abs() < 1e-6);

        // Half the debt is repaid after a quarter second.
        tokio::time::advance(Duration::from_millis(250)).await;
        let deadline = limiter.charge(0).expect("still in debt");
        assert!((secs(deadline - Instant::now()) - 0.25).abs() < 1e-6);
        tokio::time::advance(Duration::from_millis(250)).await;
        assert_eq!(limiter.charge(0), None, "debt repaid");
    }

    #[tokio::test(start_paused = true)]
    async fn idle_refill_is_capped_at_the_burst() {
        let limiter = RateLimiter::new(Some(MIB));
        let burst = limiter.burst();
        limiter.charge(burst);
        tokio::time::advance(Duration::from_secs(10)).await;
        let deadline = limiter
            .charge(burst + 64 * KIB)
            .expect("only one burst banked");
        let expected = (64 * KIB) as f64 / MIB as f64;
        assert!((secs(deadline - Instant::now()) - expected).abs() < 1e-6);
    }

    #[tokio::test(start_paused = true)]
    async fn unlimited_never_waits_or_accrues_debt() {
        let limiter = RateLimiter::new(None);
        for _ in 0..10 {
            assert_eq!(limiter.charge(u64::MAX / 16), None);
        }
        assert_eq!(limiter.balance(), 0.0);
        assert_eq!(RateLimiter::new(Some(0)).rate(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn limiting_after_unlimited_traffic_starts_at_a_full_burst() {
        let limiter = RateLimiter::new(None);
        limiter.charge(100 * MIB);
        let rate = MIB;
        limiter.set_rate(Some(rate));
        let burst = limiter.burst();
        let deadline = limiter.charge(2 * burst).expect("past the burst");
        let wait = secs(deadline - Instant::now());
        assert!(wait <= burst as f64 / rate as f64 + 1e-6, "waited {wait}s");
    }

    #[tokio::test(start_paused = true)]
    async fn rate_change_settles_the_old_refill_first() {
        let limiter = RateLimiter::new(Some(MIB));
        limiter.charge(limiter.burst() + MIB / 2); // 512 KiB of debt
        tokio::time::advance(Duration::from_millis(250)).await; // repays 256 KiB at 1 MiB/s
        limiter.set_rate(Some(2 * MIB));
        // 256 KiB left at 2 MiB/s. Refilling the whole 250 ms at the new rate
        // would wrongly have cleared the debt.
        let deadline = limiter.charge(0).expect("debt remains");
        assert!((secs(deadline - Instant::now()) - 0.125).abs() < 1e-6);
    }

    #[tokio::test(start_paused = true)]
    async fn lowering_the_rate_clamps_banked_tokens() {
        let limiter = RateLimiter::new(Some(4 * MIB)); // 1 MiB banked
        limiter.set_rate(Some(MIB)); // burst now 256 KiB
        assert_eq!(limiter.balance(), (256 * KIB) as f64);
        assert!(limiter.charge(256 * KIB + 1).is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn going_unlimited_ends_any_wait() {
        let limiter = RateLimiter::new(Some(MIB));
        assert!(limiter.charge(10 * MIB).is_some());
        limiter.set_rate(None);
        assert_eq!(limiter.charge(0), None);
        limiter.set_rate(Some(0));
        assert_eq!(limiter.rate(), None);
    }

    #[test]
    fn parse_rate_accepts_suffixes_and_fractions() {
        assert_eq!(parse_rate("1024"), Ok(1024));
        assert_eq!(parse_rate("500K"), Ok(500 * KIB));
        assert_eq!(parse_rate("500k"), Ok(500 * KIB));
        assert_eq!(parse_rate("2m"), Ok(2 * MIB));
        assert_eq!(parse_rate("1.5M"), Ok(3 * MIB / 2));
        assert_eq!(parse_rate("1G"), Ok(1024 * MIB));
    }

    #[test]
    fn parse_rate_rejects_bad_input() {
        for bad in [
            "",
            "0",
            "0K",
            "0.0001",
            "-1",
            "5X",
            "K",
            "1.",
            ".5M",
            "1.5.2",
            " 1K",
            "1 K",
            "1KB",
            "+1",
            "99999999999999999999",
            "18014398509481984G",
        ] {
            assert!(parse_rate(bad).is_err(), "{bad:?} should be rejected");
        }
    }
}
