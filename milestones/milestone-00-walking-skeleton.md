---
id: M0
title: Walking skeleton
status: todo
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
- Live smoke test against a self-hosted asset (GitHub Releases on this repo),
  marked `#[ignore]` so it runs only on demand.
- CI: GitHub Actions running fmt + clippy + test (ubuntu + windows matrix).
- `.gitattributes` enforcing LF.

**Out (deferred):**
- Pause/resume/cancel, resume-after-restart (→ M1).
- Multi-segment downloads (→ M3).
- Retry/backoff (→ M1).

## Tasks
- [ ] `cargo init` binary crate; add deps (`tokio`, `reqwest`, `clap`,
      `indicatif`, `anyhow`/`thiserror`).
- [ ] `engine::download(url, dest) -> Result<Summary>` streaming to disk.
- [ ] Chunk loop structured so rate can be throttled and the download
      paused/cancelled later (hooks only, not implemented).
- [ ] Filename resolution (URL + `Content-Disposition`), redirects, status.
- [ ] Progress reporting surfaced to the CLI via `indicatif`.
- [ ] CLI wiring: `rdm <url>`.
- [ ] Test fixture: local HTTP server helper for integration tests.
- [ ] Integration test: download from local server, assert byte-identical.
- [ ] CLI smoke test: spawn the binary, assert exit 0 + file exists + size.
- [ ] `#[ignore]` live smoke test against the self-hosted asset + checksum.
- [ ] `.github/workflows/ci.yml` (fmt, clippy -D warnings, test; ubuntu+windows).
- [ ] `.gitattributes` with `* text=auto eol=lf`.

## Acceptance criteria
- [ ] `cargo build` succeeds.
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy -- -D warnings` passes.
- [ ] `cargo test` passes offline, including the local-server integration test
      (bytes on disk match the served file) and the CLI smoke test.
- [ ] `cargo test -- --ignored` downloads the self-hosted asset and its
      checksum matches.
- [ ] CI is green on a pushed branch (fmt + clippy + test on ubuntu & windows).
- [ ] No download logic lives in the CLI layer (engine/CLI split holds).
