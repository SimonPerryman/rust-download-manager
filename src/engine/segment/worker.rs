//! A worker: one lane claiming pieces and streaming each into the `.part`.

use reqwest::header;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::watch;

use super::pool::{Leave, Slot};
use super::range::check_ranged;
use super::severity::Severity;
use super::table::Claim;
use super::wording::{claim_line, say_left};
use super::{Failure, Shared};
use crate::engine::lock::lock;
use crate::engine::{human, Error, Flow};

/// How a worker's fetch of one segment ended (short of failing).
enum Outcome {
    /// The segment is fully written.
    Done,
    /// The stop signal was raised.
    Stopped,
    /// The worker left the pool mid-segment; the segment needs a new owner.
    Retired(Leave),
}

/// A worker: repeatedly claim a segment (queued, else stolen) and fetch it,
/// until there is no work left, the stop signal is raised, or the pool is over
/// target. A failed or retired-from segment is handed back to the queue for
/// the remaining workers.
pub(super) async fn worker(
    shared: &Shared<'_>,
    first: Option<reqwest::Response>,
    mut slot: Slot<'_>,
) -> (u8, Result<(), Failure>) {
    let lane = slot.lane;
    let result = work(shared, first, &mut slot).await;
    // However it ended, the lane is no longer on any segment.
    lock(&shared.table).release(lane);
    (lane, result)
}

/// [`worker`]'s loop, on `slot`'s lane.
async fn work(
    shared: &Shared<'_>,
    first: Option<reqwest::Response>,
    slot: &mut Slot<'_>,
) -> Result<(), Failure> {
    let mut stop = shared.stop.subscribe();
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(shared.part)
        .await?;
    let mut first = first.map(|resp| (0, resp));
    // Until its first claim, a worker may still be opening the file while a
    // sibling's segment is handed back after a 429.
    let mut fresh = first.is_none();
    let number = slot.number();
    let mut finished = None;
    tracing::debug!("Lane {number} opened");
    loop {
        if shared.is_stopping() {
            return Ok(());
        }
        let (idx, resp) = match first.take() {
            Some((idx, resp)) => {
                // The probe's response already holds this piece, so no claim
                // logs it; say so the same way.
                let (seg, count) = {
                    let table = lock(&shared.table);
                    (table.segments[idx], table.segments.len())
                };
                tracing::debug!(
                    "{}",
                    claim_line(number, None, idx, count, seg, Claim::Fresh)
                );
                (idx, Some(resp))
            }
            None => {
                let leave = slot.try_retire();
                if leave != Leave::Stay {
                    say_left(number, leave);
                    tracing::debug!("Lane {number} closed");
                    return Ok(());
                }
                let claimed = {
                    let mut table = lock(&shared.table);
                    table
                        .claim_for(shared.min, fresh, slot.lane)
                        .map(|(idx, how)| (idx, how, table.segments[idx], table.segments.len()))
                };
                let Some((idx, how, seg, count)) = claimed else {
                    tracing::debug!("Lane {number} done — nothing left to fetch");
                    return Ok(());
                };
                tracing::debug!("{}", claim_line(number, finished, idx, count, seg, how));
                (idx, None)
            }
        };
        fresh = false;
        match fetch_segment(shared, &mut file, idx, resp, &mut stop, slot).await {
            Ok(Outcome::Done) => finished = Some(idx),
            Ok(Outcome::Stopped) => return Ok(()),
            Ok(Outcome::Retired(leave)) => {
                let at = {
                    let mut table = lock(&shared.table);
                    table.owner[idx] = None;
                    let seg = table.segments[idx];
                    seg.start + seg.done
                };
                say_left(number, leave);
                tracing::debug!(
                    "Lane {number} closed; piece {} goes back in the queue at {}",
                    idx + 1,
                    human::bytes(at)
                );
                return Ok(());
            }
            Err(failure) => {
                let mut table = lock(&shared.table);
                table.owner[idx] = None;
                // Under the same lock as the hand-back, so no fresh worker
                // can claim the segment in between. This worker's slot is
                // still live, hence `> 1` for "others live".
                let others_live = shared.pool.live() > 1;
                if matches!(&failure, Failure::Engine(e)
                    if matches!(Severity::of(e, others_live), Severity::RateLimit(_)))
                {
                    table.rate_limited = true;
                }
                return Err(failure);
            }
        }
    }
}

/// Fetch segment `idx` until complete, stopped or retired. A response that
/// ends early after making progress is re-requested in place; one that made
/// none is [`Error::Incomplete`].
async fn fetch_segment(
    shared: &Shared<'_>,
    file: &mut tokio::fs::File,
    idx: usize,
    mut first: Option<reqwest::Response>,
    stop: &mut watch::Receiver<bool>,
    slot: &mut Slot<'_>,
) -> Result<Outcome, Failure> {
    loop {
        let (pos, end) = {
            let table = lock(&shared.table);
            let seg = table.segments[idx];
            (seg.start + seg.done, seg.end)
        };
        if pos >= end {
            return Ok(Outcome::Done);
        }
        let resp = match first.take() {
            Some(resp) => resp,
            None => {
                if shared.is_stopping() {
                    return Ok(Outcome::Stopped);
                }
                let mut request = shared
                    .client
                    .get(&shared.url)
                    .header(header::RANGE, format!("bytes={pos}-{}", end - 1));
                if let Some(validator) = &shared.validator {
                    request = request.header(header::IF_RANGE, validator);
                }
                let resp = tokio::select! {
                    resp = request.send() => resp?,
                    _ = tokio::time::sleep(shared.read_timeout) => return Err(Error::TimedOut.into()),
                    _ = stop.wait_for(|stopped| *stopped) => return Ok(Outcome::Stopped),
                };
                tracing::debug!(
                    "Lane {} requested bytes {pos}–{} · HTTP {}",
                    slot.number(),
                    end - 1,
                    resp.status().as_u16()
                );
                check_ranged(&resp, pos, end, shared.total, shared.redirected)?;
                let reconnected =
                    std::mem::take(&mut lock(&shared.lane_failures)[usize::from(slot.lane)]);
                if reconnected > 0 {
                    tracing::info!("Lane {} reconnected", slot.number());
                }
                resp
            }
        };

        file.seek(std::io::SeekFrom::Start(pos)).await?;
        let mut body = resp.bytes_stream();
        let mut progressed = false;
        loop {
            let next = tokio::select! {
                next = crate::engine::next_chunk(&mut body, shared.read_timeout) => next,
                _ = stop.wait_for(|stopped| *stopped) => return Ok(Outcome::Stopped),
            };
            let Some(chunk) = next else { break };
            let chunk = chunk?;

            // Control point: honoured once per chunk, before the write.
            if shared.control.poll() != Flow::Continue {
                shared.raise_stop();
                return Ok(Outcome::Stopped);
            }
            // The tail may have been stolen since the request was sent.
            let room = {
                let table = lock(&shared.table);
                table.segments[idx].remaining()
            };
            let n = (chunk.len() as u64).min(room);
            file.write_all(&chunk[..n as usize]).await?;
            file.flush().await?;
            // `done` only advances once the bytes are written, so a checkpoint
            // snapshot never claims data that isn't in the file.
            let complete = {
                let mut table = lock(&shared.table);
                let seg = &mut table.segments[idx];
                seg.done = (seg.done + n).min(seg.len());
                seg.is_complete()
            };
            progressed |= n > 0;

            // Throttle point: sleeps off rate-limit debt. A sibling's failure
            // raises `stop` without touching `Control`, so watch it too.
            let stopped = tokio::select! {
                _ = shared.control.throttle(n as usize, Some(&shared.waited)) => false,
                _ = stop.wait_for(|stopped| *stopped) => true,
            };
            // Retire point: only after the chunk is written and `done`
            // advanced, so the handed-back segment resumes exactly there.
            let outcome = if stopped {
                Some(Outcome::Stopped)
            } else if complete {
                Some(Outcome::Done)
            } else {
                match slot.try_retire() {
                    Leave::Stay => None,
                    leave => Some(Outcome::Retired(leave)),
                }
            };
            shared.progressed.notify_one();
            if let Some(outcome) = outcome {
                return Ok(outcome);
            }
        }
        if !progressed {
            return Err(Error::Incomplete(format!("bytes {pos}-{} ended early", end - 1)).into());
        }
    }
}
