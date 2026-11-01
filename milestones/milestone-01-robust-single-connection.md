---
id: M1
title: Robust single-connection downloads (MVP core)
status: done
depends-on: [M0]
tags: [engine, network, persistence, testing]
---

## Goal
A single-connection download you can trust and interrupt: pause, resume,
cancel, resume-after-restart, retry, and integrity checking.

## Context / rationale
This is the real MVP core. Resume-after-restart is the signature feature and
the main driver of the persistable-state design principle. See [[roadmap]].
**MVP ships at the end of this milestone (M0 + M1): lean, single-connection,
CLI-only.**

## Scope
**In:**
- Pause / resume / cancel a running download (uses the M0 chunk-loop hooks).
- Resume after a full restart via HTTP `Range` + `Accept-Ranges`.
- Persist partial-download state: URL, byte offset, ETag/Last-Modified, target
  path, expected size.
- Validate the server resource hasn't changed before resuming (ETag/
  Last-Modified); restart cleanly if it has.
- Retry with exponential backoff on transient failures.
- Integrity: final size matches expected; optional checksum verification
  (`sha2`).
- Filename collision handling.
- Graceful handling of servers that don't support ranges or lie about it.

**Out (deferred):**
- Multi-segment parallel downloads (→ M3).
- Queue / concurrency / speed limiting (→ M4).
- GUI (→ M2).

## Tasks
- [x] Define the persistable download-state model + on-disk format (sidecar
      file alongside the partial download).
- [x] Pause/cancel via the engine's control hooks; flush state on pause.
- [x] Resume logic: read state, issue `Range` request from offset, append.
- [x] Resource-changed detection (ETag/Last-Modified mismatch → restart).
- [x] Retry/backoff wrapper around transient network/5xx failures.
- [x] Integrity check (size always; checksum when provided).
- [x] Filename collision strategy (e.g. `file (1).ext`).
- [x] Fixture cases: server without `Accept-Ranges`; server advertising ranges
      but ignoring `Range`; mid-stream disconnect.

## Acceptance criteria
_Standing gates (fmt/clippy/test/CI) live in AGENTS.md's Definition of Done and
apply here without being restated._
- [x] Kill the process mid-download, restart, and the resulting file is
      byte-identical to a full download (verified by checksum).
      (`tests/cli.rs::kill_mid_download_then_restart_is_byte_identical`)
- [x] Pausing then resuming produces a byte-identical file.
      (`tests/resume.rs::pause_then_resume_is_byte_identical`)
- [x] A server that doesn't support ranges falls back to a clean restart (no
      corruption). (`tests/resume.rs::norange_server_restarts_cleanly`)
- [x] A server that lies about range support is detected; the file is not
      silently corrupted. (`tests/resume.rs::range_liar_does_not_corrupt`)
- [x] Transient failures are retried with backoff and the download completes.
      (`tests/resume.rs::flaky_server_retries_to_completion`)
