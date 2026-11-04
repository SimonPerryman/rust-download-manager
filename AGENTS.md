# AGENTS.md — rdm

A download manager in Rust. Keep this file lean; it's the
instruction sheet for any agent (and human) working in this repo.

## Toolchain
- Rust stable (MSVC on Windows). Edition 2021+.
- **The toolchain is pinned in `rust-toolchain.toml`; CI installs from that
  file and never names a version.** Why: newer clippy adds lints, so an
  unpinned local toolchain passes code CI rejects. Bump the pin in its own
  commit and fix any new lints there.
- On Windows, build from a Developer PowerShell for VS (MSVC env loaded) so the
  linker (`link.exe`) and the Windows SDK are on PATH.

## Commands
- Build: `cargo build`
- Test (fast, offline — the default gate): `cargo test`
- Live smoke test (real network, opt-in): `cargo test -- --ignored`
- Lint (treat warnings as errors): `cargo clippy -- -D warnings`
- Format check: `cargo fmt --check`
- Run the CLI: `cargo run -- <url>`

## Architecture (non-negotiable)
- **Engine / frontend split.** Core download logic lives in a library-style
  `engine` module with a clean API. The CLI (and later any GUI) is a thin
  frontend that only calls the engine. No download logic in the frontend.
- The engine's chunk loop is written so a download's rate can be throttled and
  it can be paused/cancelled mid-stream, even before those features exist.

## Testing philosophy
- **Default tests are deterministic and offline:** drive the engine against a
  local in-process HTTP server (fixtures), never the real network.
- **A real end-to-end smoke test exists** against a public asset pinned to an
  immutable commit SHA, marked `#[ignore]` so it only runs on demand / in a
  dedicated CI job — keeps the default gate fast and reliable.
- Every feature ships with a test. Acceptance criteria must be runnable, not
  "looked fine".
- **When a test asserts ordering or overlap ("A was running while B�"), assert
  on Manager status or a fixture-controlled gate (`/gated`), not on progress
  bytes.** Why: progress is reported after each chunk's throttle wait, so
  "has bytes yet" lags reality and flakes on slow CI runners.
- **When an e2e spec types text, `browser.keys()` gets only `Key.*` constants
  and literals with no repeated consecutive character; type anything else
  (`"0.0001"`, any variable) with element send-keys (`addValue`/`setValue`).**
  Why: WebKitGTK's driver drops repeated characters sent as key actions —
  `browser.keys("0.0001")` arrived as `"0.01"` — and only the Linux CI job
  sees it. Enforced by `gui/scripts/guard-e2e-webkit.mjs` (part of `pnpm lint`).
- **When an e2e spec waits for an element to disappear, use `waitForGone()`
  from `gui/e2e/helpers.ts`, never `waitForExist({ reverse: true })`.** Why:
  WebKitGTK's driver throws a stale-element error when the element detaches
  mid-check, so reverse waits flake on the Linux CI job. Enforced by the same
  guard, `gui/scripts/guard-e2e-webkit.mjs`.

## Comments & docs
- Doc-comment (`///`) the public engine API: it's the contract and renders in
  `cargo doc`. One or two lines — say *what* and *why*, not *how*.
- Inline comments earn their place only for non-obvious *why* (e.g. the chunk
  loop's pause/throttle hook points). Don't narrate what the code already says.
- Prefer clear names over comments.
- Keep `README.md` current with user-facing behaviour (install + CLI usage)
  when it changes.

## Definition of Done (every change & milestone)
Standing gates — established in M0, apply to all work, and deliberately NOT
repeated in each milestone's acceptance criteria:
- `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` pass.
- CI is green.
- New/changed behaviour ships with a runnable test; the engine/CLI split holds.

## Milestones
Roadmap index: `roadmap.md`. Each milestone is a ticket in `milestones/`
(`milestone-NN-*.md`) with runnable acceptance criteria. Template:
`milestones/_template.md`.
