---
id: M3
title: Multi-segment parallel downloads
status: done
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
- Dynamic segment strategy: static equal split + work-stealing (an idle
  worker halves the largest remaining segment).

**Out (deferred):**
- Global/per-download speed limiting (→ M4).
- Queue (→ M4).
- Throughput-driven connection ramping (adding/removing connections from
  measured speed) (→ M4). M3's planner sizes segments from file size,
  `--connections` and a minimum segment size; work-stealing covers uneven
  connection speeds.

## Tasks
- [x] Segment planner: decide count/size from file size + connection count
      (measured-throughput ramping deferred to M4; work-stealing instead).
- [x] Pre-allocate the target file; assign each worker an offset range.
- [x] Concurrent workers writing to their offsets (`futures`).
- [x] Per-segment progress aggregated into one overall progress readout.
- [x] Per-segment resume state persisted; resume re-requests only missing ranges.
- [x] Range-support probe; clean fallback to single-connection (reuses M1).
- [x] Fixture: multi-range server; server that ignores `Range`; mid-segment
      disconnect.

## Acceptance criteria
_Standing gates (fmt/clippy/test/CI) live in AGENTS.md's Definition of Done and
apply here without being restated._
- [x] A large file downloaded multi-segment is byte-identical to a
      single-connection download (checksum match).
      (`tests/segmented.rs::multi_segment_matches_single_connection`)
- [x] Multi-segment download is measurably faster than single-connection on a
      range-supporting fixture (throughput assertion).
      (`tests/segmented.rs::multi_segment_is_faster_than_single_connection`)
- [x] Kill a multi-segment download mid-flight, restart, and the final file is
      byte-identical (only missing ranges are re-fetched).
      (`tests/cli.rs::kill_multi_segment_download_then_restart_fetches_only_missing_ranges`,
      `tests/segmented.rs::pause_then_resume_refetches_only_missing_ranges`)
- [x] A non-range server transparently falls back to single-connection and
      still completes correctly.
      (`tests/segmented.rs::server_without_accept_ranges_stays_single_connection`,
      `norange_server_stays_single_connection`,
      `range_liar_falls_back_to_single_connection`,
      `liar_without_validator_stays_single_connection`)
