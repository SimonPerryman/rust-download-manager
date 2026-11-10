//! What the log says about lanes: the lines for claims, leaves and failures.

use super::pool::Leave;
use super::table::{Claim, Segment};
use crate::engine::{human, Error};

/// What happened to a lane, e.g. `lost connection`, `got HTTP 500`.
pub(super) fn lane_problem(e: &Error) -> String {
    match e {
        Error::TimedOut => "timed out".to_owned(),
        Error::Http(e) if e.is_timeout() => "timed out".to_owned(),
        Error::Http(e) if e.is_connect() => "couldn't connect".to_owned(),
        Error::Status(code) | Error::Throttled { status: code, .. } => format!("got HTTP {code}"),
        Error::RedirectExpired(_) => "found the download link expired".to_owned(),
        _ => "lost connection".to_owned(),
    }
}

/// Say that lane `number` left the pool in another lane's place, if it did.
pub(super) fn say_left(number: u8, leave: Leave) {
    if let Leave::InsteadOf(bottom) = leave {
        tracing::info!(
            "Lane {} is still busy, so Lane {number} closes instead",
            bottom + 1
        );
    }
}

/// What lane `number` says when it claims segment `idx` of `count` (`seg` as
/// it is now), having last finished segment `finished`, if any.
pub(super) fn claim_line(
    number: u8,
    finished: Option<usize>,
    idx: usize,
    count: usize,
    seg: Segment,
    how: Claim,
) -> String {
    let piece = idx + 1;
    let range = format!("{} – {}", human::bytes(seg.start), human::bytes(seg.end));
    let lead = match finished {
        Some(f) => format!("Lane {number} finished piece {}, now on", f + 1),
        None => format!("Lane {number} started"),
    };
    match how {
        Claim::Fresh => format!("{lead} piece {piece} of {count} ({range})"),
        Claim::HandedBack => format!(
            "{lead} piece {piece} of {count} from {}",
            human::bytes(seg.start + seg.done)
        ),
        Claim::Split { victim, from } => {
            let whose = match from {
                Some(lane) => format!("Lane {}'s piece", lane + 1),
                None => "piece".to_owned(),
            };
            format!(
                "Lane {number} took over the back half of {whose} {} ({range}) as piece {piece}",
                victim + 1
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: u64, end: u64, done: u64) -> Segment {
        Segment { start, end, done }
    }

    #[test]
    fn claim_lines_name_the_piece_and_where_it_starts() {
        let mib = 1024 * 1024;
        let piece = seg(2 * mib, 4 * mib, 0);
        assert_eq!(
            claim_line(2, None, 1, 4, piece, Claim::Fresh),
            "Lane 2 started piece 2 of 4 (2.00 MB – 4.00 MB)"
        );
        assert_eq!(
            claim_line(
                1,
                Some(0),
                1,
                4,
                seg(2 * mib, 4 * mib, mib),
                Claim::HandedBack
            ),
            "Lane 1 finished piece 1, now on piece 2 of 4 from 3.00 MB"
        );
        let split = Claim::Split {
            victim: 3,
            from: Some(1),
        };
        assert_eq!(
            claim_line(1, Some(2), 4, 5, seg(7 * mib, 8 * mib, 0), split),
            "Lane 1 took over the back half of Lane 2's piece 4 (7.00 MB – 8.00 MB) as piece 5"
        );
    }
}
