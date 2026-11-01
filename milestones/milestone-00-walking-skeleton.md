---
id: M0
title: Walking skeleton
status: done
depends-on: []
tags: [engine, cli, network, testing, ci]
---

## Goal
Download one file end-to-end from the CLI, with the project's test/CI factory
in place from day one.

## Context / rationale
Proves the whole pipeline (HTTP → stream → disk) and front-loads the things
that make the project agent-buildable: a deterministic test server, a real
end-to-end smoke test, and CI. See [[roadmap]]. Establishes the engine/CLI
split that every later milestone depends on.

## Scope
**In:**
- `cargo` binary crate with an internal `engine` module (engine/CLI split).
- HTTP/HTTPS GET, stream the body to disk chunk-by-chunk (no full buffering).
- Follow redirects, sane User-Agent, handle common status codes.
- Parse `Content-Length`; derive filename from URL / `Content-Disposition`.
- Progress readout: bytes, %, speed (sliding window), ETA.
- CLI (`clap`): `rdm <url>`.
- Deterministic test fixture: in-process HTTP server (`wiremock`/`httptest`/
  small `axum`) used by integration tests — never the real network.
- Live smoke test against a public asset pinned to an immutable commit SHA
  (a file on microsoft/TypeScript), marked `#[ignore]` so it runs only on demand.
- CI: GitHub Actions running fmt + clippy + test (ubuntu + windows matrix).
- `.gitattributes` enforcing LF.

**Out (deferred):**
- Pause/resume/cancel, resume-after-restart (→ M1).
- Multi-segment downloads (→ M3).
- Retry/backoff (→ M1).

## Tasks
- [x] `cargo init` binary crate; add deps (`tokio`, `reqwest`, `clap`,
      `indicatif`, `anyhow`/`thiserror`).
- [x] `engine::download(url, dest) -> Result<Summary>` streaming to disk.
- [x] Chunk loop structured so rate can be throttled and the download
      paused/cancelled later (hooks only, not implemented).
- [x] Filename resolution (URL + `Content-Disposition`), redirects, status.
- [x] Progress reporting surfaced to the CLI via `indicatif`.
- [x] CLI wiring: `rdm <url>`.
- [x] Test fixture: local HTTP server helper for integration tests.
- [x] Integration test: download from local server, assert byte-identical.
- [x] CLI smoke test: spawn the binary, assert exit 0 + file exists + size.
- [x] `#[ignore]` live smoke test against the pinned public asset + checksum.
- [x] `.github/workflows/ci.yml` (fmt, clippy -D warnings, test; ubuntu+windows).
- [x] `.gitattributes` with `* text=auto eol=lf`.

## Acceptance criteria
- [x] `cargo build` succeeds.
- [x] `cargo fmt --check` passes.
- [x] `cargo clippy -- -D warnings` passes.
- [x] `cargo test` passes offline, including the local-server integration test
      (bytes on disk match the served file) and the CLI smoke test.
- [x] `cargo test -- --ignored` downloads the pinned public asset and its
      checksum matches.
- [x] CI is green on a pushed branch (fmt + clippy + test on ubuntu & windows).
- [x] No download logic lives in the CLI layer (engine/CLI split holds).
