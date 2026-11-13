//! The lane pool: numbered worker slots, and which lane leaves when the pool sheds.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::LANES;
use crate::engine::lock::lock;

/// Live-worker accounting: how many workers run, how many the ramp wants, and
/// whether the pool changed (or a worker was warming up) since the last ramp
/// window.
pub(super) struct Pool {
    live: AtomicU8,
    target: AtomicU8,
    changed: AtomicBool,
    /// Lane numbers in use and when the pool went over target. `live` only
    /// changes under this lock, so a leave is judged against a stable pool.
    lanes: Mutex<LaneSet>,
    /// How long the bottom lane gets to leave before any lane may; also how
    /// long a worker without bytes counts as warming up rather than stuck.
    grace: Duration,
}

#[derive(Debug)]
struct LaneSet {
    /// One bit per lane number in use; a lane's number is what the log calls it by.
    used: u32,
    /// When a leave first fell due, while one is pending.
    over_since: Option<Instant>,
    /// When each lane's worker joined, until it delivers its first bytes.
    warming_since: [Option<Instant>; LANES],
    /// Whether a worker delivered its first bytes since the last ramp window.
    warmed: bool,
}

impl Default for LaneSet {
    fn default() -> Self {
        Self {
            used: 0,
            over_since: None,
            warming_since: [None; LANES],
            warmed: false,
        }
    }
}

impl LaneSet {
    /// The highest lane number in use (zero-based).
    fn bottom(&self) -> Option<u8> {
        (self.used != 0).then(|| (31 - self.used.leading_zeros()) as u8)
    }
}

impl Pool {
    pub(super) fn new(target: u8, grace: Duration) -> Self {
        Self {
            live: AtomicU8::new(0),
            target: AtomicU8::new(target),
            // The first window holds the initial spawn.
            changed: AtomicBool::new(true),
            lanes: Mutex::new(LaneSet::default()),
            grace,
        }
    }

    /// Count a new worker in, on lane `prefer` if it's free, else the lowest
    /// free one, warming up until [`Slot::delivered`]; it is counted out when
    /// the slot drops.
    pub(super) fn join(&self, prefer: Option<u8>) -> Slot<'_> {
        let mut lanes = lock(&self.lanes);
        let used = lanes.used;
        let free = |lane: &u8| usize::from(*lane) < LANES && used & (1 << lane) == 0;
        let lane = prefer
            .filter(free)
            .or_else(|| (0..LANES as u8).find(free))
            .unwrap_or(LANES as u8 - 1);
        lanes.used |= 1 << lane;
        lanes.warming_since[usize::from(lane)] = Some(Instant::now());
        self.live.fetch_add(1, Ordering::SeqCst);
        self.changed.store(true, Ordering::SeqCst);
        Slot {
            pool: self,
            held: true,
            warming: true,
            lane,
        }
    }

    /// Count `lane`'s worker out.
    fn leave(&self, lanes: &mut LaneSet, lane: u8) {
        lanes.used &= !(1 << lane);
        lanes.warming_since[usize::from(lane)] = None;
        self.live.fetch_sub(1, Ordering::SeqCst);
        self.changed.store(true, Ordering::SeqCst);
    }

    /// The highest lane number in use (zero-based): the next to be shed.
    pub(super) fn bottom(&self) -> Option<u8> {
        lock(&self.lanes).bottom()
    }

    pub(super) fn live(&self) -> u8 {
        self.live.load(Ordering::SeqCst)
    }

    pub(super) fn target(&self) -> u8 {
        self.target.load(Ordering::SeqCst)
    }

    pub(super) fn set_target(&self, n: u8) {
        self.target.store(n, Ordering::SeqCst);
    }

    pub(super) fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::SeqCst)
    }

    /// Whether any worker was warming up since the last call: one delivered
    /// its first bytes in between, or one is still waiting on them. A worker
    /// silent for the grace is stuck, not slow to connect, so it no longer
    /// counts and the ramp can judge (and shed) it.
    pub(super) fn take_warming(&self) -> bool {
        self.take_warming_at(Instant::now())
    }

    /// [`Pool::take_warming`] at `now`.
    fn take_warming_at(&self, now: Instant) -> bool {
        let mut lanes = lock(&self.lanes);
        let warming = lanes
            .warming_since
            .iter()
            .flatten()
            .any(|&since| now.saturating_duration_since(since) < self.grace);
        std::mem::take(&mut lanes.warmed) || warming
    }
}

/// Whether a worker left an over-target pool, and in whose place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Leave {
    /// It stays: the pool isn't over target, or it isn't this lane's turn.
    Stay,
    /// It left as the highest-numbered lane.
    Bottom,
    /// It left because the bottom lane (zero-based) didn't within the grace.
    InsteadOf(u8),
}

/// One worker's place in the [`Pool`].
pub(super) struct Slot<'a> {
    pool: &'a Pool,
    held: bool,
    /// No bytes delivered yet (and still in the pool).
    warming: bool,
    /// Zero-based; the log shows it one-based.
    pub(super) lane: u8,
}

impl Slot<'_> {
    /// Leave the pool if it is over target and it is this lane's turn.
    pub(super) fn try_retire(&mut self) -> Leave {
        self.try_retire_at(Instant::now())
    }

    /// [`Slot::try_retire`] at `now`. The bottom lane leaves first, so Lane 1
    /// stays; once a leave has been due for the pool's grace, another lane may
    /// go, so a bottom lane stuck on the server can't hold the shed up. Lane 1
    /// goes instead only when it is the one other lane.
    fn try_retire_at(&mut self, now: Instant) -> Leave {
        let pool = self.pool;
        let mut lanes = lock(&pool.lanes);
        let live = pool.live();
        if !self.held || live <= pool.target() {
            lanes.over_since = None;
            return Leave::Stay;
        }
        let since = *lanes.over_since.get_or_insert(now);
        let bottom = lanes.bottom().unwrap_or(self.lane);
        let leave = if self.lane == bottom {
            Leave::Bottom
        } else if now.saturating_duration_since(since) >= pool.grace
            && (self.lane != 0 || lanes.used.count_ones() <= 2)
        {
            Leave::InsteadOf(bottom)
        } else {
            return Leave::Stay;
        };
        pool.leave(&mut lanes, self.lane);
        lanes.over_since = (live - 1 > pool.target()).then_some(now);
        self.held = false;
        self.warming = false;
        leave
    }

    /// The worker delivered bytes: it is no longer warming up.
    pub(super) fn delivered(&mut self) {
        if std::mem::take(&mut self.warming) {
            let mut lanes = lock(&self.pool.lanes);
            lanes.warming_since[usize::from(self.lane)] = None;
            lanes.warmed = true;
        }
    }

    /// The lane's number as the log shows it, from 1.
    pub(super) fn number(&self) -> u8 {
        self.lane + 1
    }
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        if self.held {
            let mut lanes = lock(&self.pool.lanes);
            self.pool.leave(&mut lanes, self.lane);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::segment::SHED_GRACE;

    /// Lanes get the lowest free number, a replacement may keep its
    /// predecessor's, and a number is free again once its worker leaves.
    #[test]
    fn lanes_are_numbered_and_reused() {
        let pool = Pool::new(3, SHED_GRACE);
        let mut slots: Vec<_> = (0..3).map(|_| pool.join(None)).collect();
        assert_eq!(
            slots.iter().map(Slot::number).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(pool.join(Some(1)).number(), 4, "lane 2 is taken");
        drop(slots.remove(1));
        assert_eq!(pool.join(None).number(), 2, "the lowest free lane");
        drop(slots.remove(0));
        let kept = pool.join(Some(0));
        assert_eq!(kept.number(), 1, "a replacement keeps its lane");
        pool.set_target(1);
        assert_eq!(slots[0].try_retire(), Leave::Bottom, "lane 3 is the bottom");
        assert_eq!(
            pool.join(Some(2)).number(),
            3,
            "a retired lane is free again"
        );
    }

    #[test]
    fn retire_leaves_only_while_over_target_and_counts_once() {
        let pool = Pool::new(3, SHED_GRACE);
        assert!(pool.take_changed(), "the initial spawn is a pool change");
        let mut slots: Vec<_> = (0..3).map(|_| pool.join(None)).collect();
        assert_eq!(pool.live(), 3);
        assert!(pool.take_changed());
        assert_eq!(
            slots[2].try_retire(),
            Leave::Stay,
            "at target: nobody leaves"
        );

        pool.set_target(1);
        assert_eq!(slots[2].try_retire(), Leave::Bottom);
        assert_eq!(slots[2].try_retire(), Leave::Stay, "it already left");
        assert_eq!(pool.live(), 2);
        assert!(pool.take_changed());

        // The retired slot was already counted out when it left.
        slots.remove(2);
        assert_eq!(pool.live(), 2);
        assert!(!pool.take_changed());

        // A pending retire lapses if the pool shrinks some other way.
        slots.remove(1);
        assert_eq!(slots[0].try_retire(), Leave::Stay);
        drop(slots);
        assert_eq!(pool.live(), 0);
    }

    /// A worker warms up from joining until its first bytes, and the window
    /// those land in still counts; leaving without any isn't a warm-up.
    #[test]
    fn a_worker_warms_up_until_its_first_bytes() {
        let pool = Pool::new(2, SHED_GRACE);
        assert!(!pool.take_warming(), "no workers yet");
        let mut first = pool.join(None);
        let second = pool.join(None);
        assert!(pool.take_warming());
        assert!(pool.take_warming(), "still no bytes");
        first.delivered();
        drop(second);
        assert!(pool.take_warming(), "the first warmed up in this window");
        assert!(!pool.take_warming(), "warm, and the other left");
        first.delivered();
        assert!(!pool.take_warming(), "only the first bytes count");
        drop(pool.join(None));
        assert!(
            !pool.take_warming(),
            "leaving without bytes isn't a warm-up"
        );
    }

    /// A worker without bytes for the grace is stuck, not warming up; one that
    /// retired before any bytes stops warming even though its slot lives on.
    #[test]
    fn warming_up_lasts_at_most_the_grace() {
        let grace = Duration::from_secs(3);
        let pool = Pool::new(1, grace);
        let before = Instant::now();
        let mut first = pool.join(None);
        let after = Instant::now();
        assert!(pool.take_warming_at(before + grace - Duration::from_millis(1)));
        assert!(!pool.take_warming_at(after + grace), "stuck, not warming");
        first.delivered();
        assert!(pool.take_warming(), "its late bytes still end a warm-up");

        let mut late = pool.join(None);
        assert!(pool.take_warming());
        assert_eq!(late.try_retire(), Leave::Bottom);
        assert!(!pool.take_warming(), "it left the pool");
        late.delivered();
        assert!(!pool.take_warming(), "a retired worker's bytes don't count");
    }

    /// The bottom lane is the one shed, so Lane 1 always stays; the others
    /// leave only once the bottom one has had the grace to and hasn't.
    #[test]
    fn the_bottom_lane_leaves_first_unless_it_is_stuck() {
        let grace = Duration::from_secs(3);
        let pool = Pool::new(3, grace);
        let mut slots: Vec<_> = (0..3).map(|_| pool.join(None)).collect();
        pool.set_target(2);
        let t0 = Instant::now();

        assert_eq!(slots[0].try_retire_at(t0), Leave::Stay, "Lane 1 stays");
        assert_eq!(slots[1].try_retire_at(t0), Leave::Stay, "Lane 2 stays");
        // Lane 3 is stuck: no turn of its own comes. Within the grace the
        // others still wait for it...
        let early = t0 + grace - Duration::from_millis(1);
        assert_eq!(slots[1].try_retire_at(early), Leave::Stay);
        // ...after it, the next lane to check leaves in its place.
        assert_eq!(slots[1].try_retire_at(t0 + grace), Leave::InsteadOf(2));
        assert_eq!(pool.live(), 2);
        assert_eq!(
            slots[0].try_retire_at(t0 + grace),
            Leave::Stay,
            "one per step"
        );
        assert_eq!(slots[2].try_retire_at(t0 + grace), Leave::Stay);
        assert_eq!(slots[2].number(), 3, "the stuck lane keeps its number");
    }

    /// Past the grace, Lane 1 still waits for a middle lane to go in the stuck
    /// bottom lane's place, and goes itself only when it is the one other lane.
    #[test]
    fn lane_one_stands_in_for_a_stuck_lane_only_as_the_last_choice() {
        let grace = Duration::from_secs(3);
        let pool = Pool::new(3, grace);
        let mut slots: Vec<_> = (0..3).map(|_| pool.join(None)).collect();
        pool.set_target(2);
        let t0 = Instant::now();
        assert_eq!(slots[0].try_retire_at(t0), Leave::Stay);
        let late = t0 + grace;
        assert_eq!(slots[0].try_retire_at(late), Leave::Stay, "Lane 2 can go");
        assert_eq!(slots[1].try_retire_at(late), Leave::InsteadOf(2));

        pool.set_target(1);
        let t1 = late + Duration::from_millis(1);
        assert_eq!(slots[0].try_retire_at(t1), Leave::Stay);
        assert_eq!(
            slots[0].try_retire_at(t1 + grace),
            Leave::InsteadOf(2),
            "only Lane 1 is left to stand in"
        );
        assert_eq!(pool.live(), 1);
    }

    /// Shedding two lanes takes the bottom two, each in turn.
    #[test]
    fn each_shed_gives_the_bottom_lane_its_own_turn() {
        let grace = Duration::from_secs(3);
        let pool = Pool::new(4, grace);
        let mut slots: Vec<_> = (0..4).map(|_| pool.join(None)).collect();
        pool.set_target(2);
        let t0 = Instant::now();
        assert_eq!(slots[3].try_retire_at(t0), Leave::Bottom);
        // Lane 3 is now the bottom, and gets the grace from when Lane 4 left.
        let early = t0 + grace - Duration::from_millis(1);
        assert_eq!(slots[0].try_retire_at(early), Leave::Stay);
        assert_eq!(slots[2].try_retire_at(early), Leave::Bottom);
        assert_eq!(pool.live(), 2);
    }
}
