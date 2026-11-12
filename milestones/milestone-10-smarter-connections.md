---
id: M10
title: Smarter connections
status: todo
depends-on: [M4]
tags: [engine, network, perf, testing]
---

## Goal
A multi-connection download keeps adjusting its connection count for the
whole download, not just the first few seconds, and a rate-limited host
costs one wait instead of one per segment.

## Context / rationale
M4's ramping (`src/engine/ramp.rs`, driven from
`src/engine/segment/mod.rs`) starts on one connection, adds one per 1 s
window while aggregate speed beats its best by 10%, then **settles for
good**. That leaves four gaps (roadmap Follow-ups), all in the same state
machine, so they're one milestone:

- Lifting or raising a speed limit never brings connections back.
- A download that slows down later never sheds a connection.
- A slow-to-connect host has its setup time judged as "no improvement".
- A host that limits request *rate* (OVH: 429 + Retry-After 10 s) charges
  that wait on every new segment request.

Roadmap index: [[roadmap]].

## Decisions (workers follow these; escalate only if one can't hold)
- **Settled is no longer final.** `Ramp` gains a way back out of settled:
  a re-probe trials one more connection, judged exactly like a ramp-up
  window (keep it on >10% gain, retire it otherwise).
- **When to re-probe:** (a) at once when the speed limit is raised or
  removed; (b) every 30 s while settled below the ceiling. Never above a
  host cap learned from a 429/503 (`cap`) for this attempt.
- **When to shed:** 3 consecutive judged windows below 70% of the best
  window → retire one connection (never below 1) and reset the best to the
  current speed, so a later re-probe can win it back. Windows that are
  frozen by the speed limit, or that see a pool change, are not judged
  (as today).
- **Warming up:** a connection counts as warming up until it has delivered
  its first bytes, not just for the window it joined; a window with any
  warming connection is not judged.
- **Rate-limited hosts:** after a 429 with Retry-After, the rest of the
  download must cost at most one more such wait. How (one open-ended
  request for the remainder, reusing a live connection, …) is the worker's
  call; the outcome is what's tested.
- Lane UX from #83 holds: a shed closes the highest lane; log lines use
  the existing wording ("Completed", lane counts via `human::count`).
- Every behaviour is proven offline with the fixture server and a short
  ramp window (`Ramping::Window`), never the real network.

## Scope
**In:** re-probe after a settle; shedding on a sustained drop; warm-up
until first bytes; one wait for rate-limited hosts; `docs/how-it-works.md`
updated.

**Out (deferred):**
- Ramp window tuning (1 s → smaller?): needs live measurements the owner
  runs; stays a roadmap follow-up.
- Changing the default connection count (4) or the 10% margin.

## Tasks
- [ ] `ramp-reprobe`
- [ ] `ramp-shed-on-drop`
- [ ] `ramp-slow-setup`
- [ ] `rate-limit-remainder`

## Acceptance criteria
- [ ] `cargo test` passes, with new fixture tests for each task.
- [ ] `docs/how-it-works.md` describes the full ramp lifecycle (ramp up,
      settle, re-probe, shed) and the rate-limit behaviour.
