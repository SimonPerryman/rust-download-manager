//! How a lane's failure bears on its attempt: rate limits, connection caps, and the rest.

use std::time::Duration;

use crate::engine::{throttle_wait, Error};

/// How a transient worker failure bears on the rest of its attempt, least to
/// most; the attempt reports its most severe one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Severity {
    /// A bare 503 while other connections are still served: a connection cap.
    Cap,
    /// Any other transient failure (a reset, a timeout, a short body).
    Other,
    /// A 429, any refusal with `Retry-After`, or a bare 503 to the last live
    /// connection; the longest wait ranks highest.
    RateLimit(Duration),
}

impl Severity {
    pub(super) fn of(e: &Error, others_live: bool) -> Self {
        match e {
            Error::Status(503) if others_live => Self::Cap,
            _ => throttle_wait(e).map_or(Self::Other, Self::RateLimit),
        }
    }
}

pub(super) fn capped(target: u8, cap: Option<u8>) -> u8 {
    cap.map_or(target, |c| target.min(c))
}

/// Record a connection cap hit with `live` connections still served; a cap
/// only ever tightens within an attempt.
pub(super) fn lower_cap(cap: Option<u8>, live: u8) -> Option<u8> {
    let live = live.max(1);
    Some(cap.map_or(live, |c| c.min(live)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limits_outrank_other_failures_and_caps_rank_lowest() {
        let of = |e: Error, others_live| Severity::of(&e, others_live);
        let after = |status, secs| Error::Throttled {
            status,
            retry_after: Duration::from_secs(secs),
        };
        let reset = || Error::Incomplete("reset".into());
        // A bare 503 alongside live connections is a cap, below anything else.
        assert_eq!(of(Error::Status(503), true), Severity::Cap);
        assert!(of(reset(), true) > of(Error::Status(503), true));
        // ...but to the last live connection it is a rate limit.
        assert_eq!(
            of(Error::Status(503), false),
            Severity::RateLimit(Duration::ZERO)
        );
        // A 429 or any Retry-After is a rate limit, live siblings or not.
        assert!(of(Error::Status(429), true) > of(reset(), true));
        assert_eq!(
            of(after(503, 5), true),
            Severity::RateLimit(Duration::from_secs(5))
        );
        // Among rate limits the longest wait wins.
        assert!(of(after(429, 30), true) > of(after(429, 5), true));
        assert!(of(after(429, 5), true) > of(Error::Status(429), true));
    }

    #[test]
    fn a_cap_outlasts_later_ramp_targets_and_only_tightens() {
        assert_eq!(capped(4, None), 4, "no cap leaves the ramp alone");
        let cap = lower_cap(None, 2);
        assert_eq!(capped(4, cap), 2, "a later ramp tick can't lift the cap");
        let cap = lower_cap(cap, 3);
        assert_eq!(cap, Some(2), "a later cap at 3 doesn't raise it");
        assert_eq!(capped(4, cap), 2);
        assert_eq!(capped(1, cap), 1, "the ramp can still go lower");
    }
}
