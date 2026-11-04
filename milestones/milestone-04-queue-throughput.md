---
id: M4
title: Queue & throughput management
status: done
depends-on: [M1]
tags: [engine, perf]
---

## Goal
Day-to-day "set it and forget it" usage: a download queue with a concurrency
cap, plus per-download and global speed limiting.

## Context / rationale
These are common day-to-day download-manager conveniences, but they're not
MVP. Queueing is a scheduler layer *above* the engine (cleanly separable);
speed limiting is a *property of the engine loop* and reuses the throttle hook
left in at M0. See [[roadmap]]. Depends on M1; benefits from M3 if present.

## Scope
**In:**
- Download queue with a configurable concurrency cap (scheduler above engine).
- Per-download speed limiting (throttle inside the engine chunk loop).
- Global speed limiting across all active downloads.
- Dynamic connection scaling based on measured speed: throughput-driven
  connection ramping for M3's multi-segment downloads (moved here from M3,
  which ships a static split + work-stealing).

**Out (deferred):**
- Scheduling by time-of-day, bandwidth schedules (backlog).
- Post-download actions (backlog).

## Tasks
- [x] Scheduler: active vs. waiting sets, honour the concurrency cap.
- [x] Per-download throttle wired into the engine chunk loop.
- [x] Global rate limiter shared across active downloads (token-bucket style).
- [x] Surface queue state + limits to the frontend(s).
- [x] (Follow-up to PR A) GUI row control to view/change/clear a download's
      own limit live, with one number + unit rate input shared by the row,
      the Add dialog and Settings.
- [x] (PR B) Throughput-driven connection ramping for multi-segment downloads
      (deferred from M3): start at 1, add while aggregate speed keeps rising,
      close the trial connection and stop ramping when it doesn't; freeze
      while rate-limited. General mid-download shedding is a follow-up.
- [x] Fixture/tests measuring effective rate against configured limits.

## Acceptance criteria
_Standing gates (fmt/clippy/test/CI) live in AGENTS.md's Definition of Done and
apply here without being restated._
- [x] Queue 10 downloads with a cap of 3: at most 3 run concurrently; the rest
      start as slots free.
- [x] A per-download speed limit is honoured within tolerance (measured rate ≤
      limit).
- [x] A global speed limit caps aggregate throughput across active downloads
      within tolerance.
- [x] Removing/pausing a queued item reschedules correctly.
- [x] Against a fixture that caps each connection at R and the total at 4R, a
      download with max 8 settles at 4 connections; with a binding speed limit
      it never ramps above 1.
      (`tests/ramping.rs::settles_at_the_fixture_aggregate_cap`,
      `tests/ramping.rs::binding_speed_limit_never_ramps`)
