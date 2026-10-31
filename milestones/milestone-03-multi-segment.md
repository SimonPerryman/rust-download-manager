---
id: M3
title: Multi-segment parallel downloads
status: todo
depends-on: [M1]
tags: [engine, network, perf]
---

## Goal
Speed up large downloads by splitting a file into byte-ranges and downloading
the segments in parallel, then stitching them into one correct file.

## Context / rationale
Deferred past the MVP because it's the most complex part (offset writing,
per-segment resume state, servers lying about range support) and the MVP works
without it. Depends on M1's range + persistable-state foundations. See
[[roadmap]]. Independent of M2 (GUI) — can be done before or after.

## Scope
**In:**
- Split a file into N byte-ranges; download concurrently (one `Range` request
  per worker).
- Offset writing into a pre-allocated file (seek + write per worker).
- Per-segment resume state so a killed multi-segment download resumes.
- Fallback to single-connection when the server doesn't support ranges.
- Dynamic segment/connection strategy based on measured throughput.

**Out (deferred):**
- Global/per-download speed limiting (→ M4).
- Queue (→ M4).

## Tasks
- [ ] Segment planner: decide count/size from file size + measured throughput.
- [ ] Pre-allocate the target file; assign each worker an offset range.
- [ ] Concurrent workers writing to their offsets (`futures`).
- [ ] Per-segment progress aggregated into one overall progress readout.
- [ ] Per-segment resume state persisted; resume re-requests only missing ranges.
- [ ] Range-support probe; clean fallback to single-connection (reuses M1).
- [ ] Fixture: multi-range server; server that ignores `Range`; mid-segment
      disconnect.

## Acceptance criteria
_Standing gates (fmt/clippy/test/CI) live in AGENTS.md's Definition of Done and
apply here without being restated._
- [ ] A large file downloaded multi-segment is byte-identical to a
      single-connection download (checksum match).
- [ ] Multi-segment download is measurably faster than single-connection on a
      range-supporting fixture (throughput assertion).
- [ ] Kill a multi-segment download mid-flight, restart, and the final file is
      byte-identical (only missing ranges are re-fetched).
- [ ] A non-range server transparently falls back to single-connection and
      still completes correctly.
