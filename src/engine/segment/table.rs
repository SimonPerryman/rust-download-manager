//! Pieces and the shared work table: planning, claiming and stealing byte ranges.

use serde::{Deserialize, Serialize};

use super::MAX_CONNECTIONS;

/// One byte range `[start, end)` of a segmented download, of which the first
/// `done` bytes are written to the `.part`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    /// First byte of the range.
    pub start: u64,
    /// One past the last byte of the range.
    pub end: u64,
    /// Bytes written from `start`; never claims more than is on disk.
    pub done: u64,
}

impl Segment {
    pub(crate) fn len(&self) -> u64 {
        self.end - self.start
    }

    pub(crate) fn remaining(&self) -> u64 {
        self.len().saturating_sub(self.done)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.done >= self.len()
    }
}

/// Split `total` bytes into equal ranges, one per connection, but none smaller
/// than `min` (the last absorbs the remainder) and at most [`MAX_CONNECTIONS`].
pub(crate) fn plan(total: u64, connections: u8, min: u64) -> Vec<Segment> {
    let n = u64::from(connections)
        .min(total / min.max(1))
        .clamp(1, u64::from(MAX_CONNECTIONS));
    let size = total / n;
    (0..n)
        .map(|i| Segment {
            start: i * size,
            end: if i == n - 1 { total } else { (i + 1) * size },
            done: 0,
        })
        .collect()
}

/// Work stealing: split the segment with the most bytes left in half, pushing
/// the back half as a new segment; returns `(victim, tail)` indices. Segments
/// with less than `2 * min` left are not worth a second connection.
pub(crate) fn steal(segments: &mut Vec<Segment>, min: u64) -> Option<(usize, usize)> {
    let threshold = min.max(1).saturating_mul(2);
    let (victim, remaining) = segments
        .iter()
        .map(Segment::remaining)
        .enumerate()
        .filter(|&(_, remaining)| remaining >= threshold)
        .max_by_key(|&(i, remaining)| (remaining, std::cmp::Reverse(i)))?;
    let seg = &mut segments[victim];
    let mid = seg.start + seg.done + remaining / 2;
    let tail = Segment {
        start: mid,
        end: seg.end,
        done: 0,
    };
    seg.end = mid;
    segments.push(tail);
    Some((victim, segments.len() - 1))
}

/// The shared work table: segments plus which lane, if any, is on each.
pub(super) struct Table {
    pub(super) segments: Vec<Segment>,
    /// The lane (zero-based) fetching each segment.
    pub(super) owner: Vec<Option<u8>>,
    /// Set once the host has refused a request as a rate limit this attempt.
    pub(super) rate_limited: bool,
}

/// How a lane came by the segment it claimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Claim {
    /// One nobody has started.
    Fresh,
    /// One another lane started and handed back.
    HandedBack,
    /// The back half of segment `victim`, split off while lane `from` was on it.
    Split { victim: usize, from: Option<u8> },
}

impl Table {
    /// [`Table::claim`] for `lane`, which first lets go of what it held; one
    /// that hasn't sent a request yet gets nothing once the host has
    /// rate-limited us, since its first request would be a new connection
    /// (the outer backoff resumes that work).
    pub(super) fn claim_for(&mut self, min: u64, fresh: bool, lane: u8) -> Option<(usize, Claim)> {
        self.release(lane);
        if fresh && self.rate_limited {
            return None;
        }
        self.claim(min, lane)
    }

    /// Hand `lane` an unowned unfinished segment, else steal one. Releasing
    /// and claiming under one lock means a lane is never seen holding nothing
    /// between two segments.
    fn claim(&mut self, min: u64, lane: u8) -> Option<(usize, Claim)> {
        let queued = (0..self.segments.len())
            .find(|&i| self.owner[i].is_none() && !self.segments[i].is_complete());
        let (idx, how) = match queued {
            Some(i) if self.segments[i].done > 0 => (i, Claim::HandedBack),
            Some(i) => (i, Claim::Fresh),
            None => {
                let (victim, tail) = steal(&mut self.segments, min)?;
                self.owner.push(None);
                let from = self.owner[victim];
                (tail, Claim::Split { victim, from })
            }
        };
        self.owner[idx] = Some(lane);
        Some((idx, how))
    }

    /// Let go of whatever `lane` holds.
    pub(super) fn release(&mut self, lane: u8) {
        for owner in &mut self.owner {
            if *owner == Some(lane) {
                *owner = None;
            }
        }
    }

    /// Each segment's lane, numbered from 1 as the log names it.
    pub(super) fn lanes(&self) -> Vec<Option<u8>> {
        self.owner.iter().map(|o| o.map(|lane| lane + 1)).collect()
    }

    pub(super) fn done(&self) -> u64 {
        self.segments.iter().map(|s| s.done).sum()
    }

    /// The lowest offset not yet on disk, if any.
    pub(super) fn first_missing(&self) -> Option<u64> {
        self.segments
            .iter()
            .filter(|s| !s.is_complete())
            .map(|s| s.start + s.done)
            .min()
    }

    /// One past the highest offset not yet on disk, if any.
    pub(super) fn last_missing_end(&self) -> Option<u64> {
        self.segments
            .iter()
            .filter(|s| !s.is_complete())
            .map(|s| s.end)
            .max()
    }

    /// The segment whose range holds byte `at` (segments tile the file).
    pub(super) fn segment_at(&self, at: u64) -> Option<usize> {
        self.segments
            .iter()
            .position(|s| (s.start..s.end).contains(&at))
    }

    /// Whether some unfinished segment has no worker (e.g. one handed back).
    pub(super) fn has_unowned_work(&self) -> bool {
        self.segments
            .iter()
            .zip(&self.owner)
            .any(|(seg, owner)| owner.is_none() && !seg.is_complete())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: u64, end: u64, done: u64) -> Segment {
        Segment { start, end, done }
    }

    #[test]
    fn plan_splits_equally_with_remainder_in_last() {
        assert_eq!(
            plan(10, 3, 1),
            vec![seg(0, 3, 0), seg(3, 6, 0), seg(6, 10, 0)]
        );
        assert_eq!(
            plan(8, 4, 2),
            vec![seg(0, 2, 0), seg(2, 4, 0), seg(4, 6, 0), seg(6, 8, 0)]
        );
    }

    #[test]
    fn plan_respects_min_size_cap_and_single_connection() {
        // Only 2 segments of at least 4 bytes fit in 9.
        assert_eq!(plan(9, 8, 4), vec![seg(0, 4, 0), seg(4, 9, 0)]);
        // Smaller than one min segment → one segment.
        assert_eq!(plan(3, 4, 4), vec![seg(0, 3, 0)]);
        // Never more than MAX_CONNECTIONS.
        assert_eq!(plan(1000, 200, 1).len(), MAX_CONNECTIONS as usize);
        assert_eq!(plan(1000, 1, 1), vec![seg(0, 1000, 0)]);
    }

    #[test]
    fn steal_halves_the_largest_remaining_segment() {
        let mut segs = vec![seg(0, 100, 90), seg(100, 200, 20), seg(200, 300, 50)];
        assert_eq!(steal(&mut segs, 10), Some((1, 3)));
        // Segment 1 has 80 left from 120: split at 160.
        assert_eq!(segs[1], seg(100, 160, 20));
        assert_eq!(segs[3], seg(160, 200, 0));
    }

    #[test]
    fn steal_refuses_segments_under_twice_min() {
        let mut segs = vec![seg(0, 100, 81), seg(100, 200, 99)];
        assert_eq!(steal(&mut segs, 10), None);
        assert_eq!(segs.len(), 2);
        // Exactly 2×min left is stealable.
        let mut segs = vec![seg(0, 100, 80)];
        assert_eq!(steal(&mut segs, 10), Some((0, 1)));
        assert_eq!(segs, vec![seg(0, 90, 80), seg(90, 100, 0)]);
    }

    #[test]
    fn claim_prefers_queued_then_steals() {
        let mut table = Table {
            segments: vec![seg(0, 100, 0), seg(100, 200, 100), seg(200, 300, 0)],
            owner: vec![Some(0), None, None],
            rate_limited: false,
        };
        // Segment 1 is complete, so the queued pick is segment 2.
        assert_eq!(table.claim(10, 1), Some((2, Claim::Fresh)));
        // Nothing queued: steal from the largest remaining (segment 0 wins ties).
        let split = Claim::Split {
            victim: 0,
            from: Some(0),
        };
        assert_eq!(table.claim(10, 2), Some((3, split)));
        assert_eq!(table.segments[0], seg(0, 50, 0));
        assert_eq!(table.owner, vec![Some(0), None, Some(1), Some(2)]);
        assert_eq!(table.lanes(), vec![Some(1), None, Some(2), Some(3)]);
    }

    /// A lane lets go of the segment it finished in the same step that hands
    /// it the next one, and a started segment nobody holds is handed back.
    #[test]
    fn a_lane_moves_from_one_segment_to_the_next() {
        let mut table = Table {
            segments: vec![seg(0, 100, 100), seg(100, 200, 40), seg(200, 300, 0)],
            owner: vec![Some(0), None, Some(1)],
            rate_limited: false,
        };
        assert_eq!(table.claim_for(10, false, 0), Some((1, Claim::HandedBack)));
        assert_eq!(table.owner, vec![None, Some(0), Some(1)]);
        table.release(1);
        assert_eq!(table.lanes(), vec![None, Some(1), None]);
    }

    #[test]
    fn first_missing_and_segment_at_follow_the_tiling() {
        let table = Table {
            segments: vec![seg(0, 100, 100), seg(200, 300, 0), seg(100, 200, 40)],
            owner: vec![None, None, None],
            rate_limited: false,
        };
        assert_eq!(table.first_missing(), Some(140));
        assert_eq!(table.last_missing_end(), Some(300));
        assert_eq!(table.segment_at(0), Some(0));
        assert_eq!(table.segment_at(199), Some(2));
        assert_eq!(table.segment_at(200), Some(1));
        assert_eq!(table.segment_at(300), None);

        let done = Table {
            segments: vec![seg(0, 100, 100)],
            owner: vec![None],
            rate_limited: false,
        };
        assert_eq!(done.first_missing(), None);
        assert_eq!(done.last_missing_end(), None);
    }

    #[test]
    fn rate_limit_holds_handed_back_work_from_fresh_workers_only() {
        let mut table = Table {
            segments: vec![seg(0, 100, 100), seg(100, 200, 0)],
            owner: vec![None, None],
            rate_limited: true,
        };
        // A worker still starting up would open a new connection: it waits
        // for the outer backoff instead.
        assert_eq!(table.claim_for(10, true, 1), None);
        assert_eq!(table.owner, vec![None, None]);
        // A live worker that finished its own segment may take it.
        assert_eq!(table.claim_for(10, false, 0), Some((1, Claim::Fresh)));

        table.rate_limited = false;
        table.owner[1] = None;
        assert_eq!(table.claim_for(10, true, 1), Some((1, Claim::Fresh)));
    }
}
