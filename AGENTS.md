# AGENTS.md — rdm

A download manager in Rust. Keep this file lean; it's the
instruction sheet for any agent (and human) working in this repo.

## Toolchain
- Rust stable (MSVC on Windows). Edition 2021+.
- **The toolchain is pinned in `rust-toolchain.toml`; CI installs from that
  file and never names a version.** Why: newer clippy adds lints, so an
  unpinned local toolchain passes code CI rejects. Bump the pin in its own
  commit and fix any new lints there.
- **CI pins everything it runs on:**
  - runner images are exact (`ubuntu-24.04`, never `*-latest`);
  - actions use a full commit SHA with a `# vX.Y.Z` comment;
  - `node-version` is an exact `x.y.z`;
  - `cargo install` passes `--version`.

  Why: supply-chain safety, because a moved tag can't inject code, and
  because an upstream move can't break CI unannounced (`ubuntu-latest`
  moving to 26.04 renamed `webkit2gtk-driver`). Apt packages are pinned by
  the image, not by version, because Ubuntu drops superseded versions,
  which breaks builds and blocks security fixes. Bump a pin in its own
  commit. Enforced by `gui/scripts/guard-ci-pins.mjs` (part of `pnpm lint`).
- On Windows, build from a Developer PowerShell for VS (MSVC env loaded) so the
  linker (`link.exe`) and the Windows SDK are on PATH.

## Commands
- Build: `cargo build`
- Test (fast, offline — the default gate): `cargo test`
  (CI runs `cargo nextest run --profile ci` + `cargo test --doc`; config in
  `.config/nextest.toml`; optional locally)
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
- **When a test asserts ordering or overlap ("A was running while B…"), assert
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
- **When an e2e spec reads text that sits inside a `truncate` (ellipsis)
  element, even through an ancestor, use `waitForText()` from
  `gui/e2e/helpers.ts`, never `getText()` or `toHaveText()`.** Why:
  WebKitGTK's driver reads ellipsis text as `""` (the toolbar `h1` and the
  finish notification's filename failed only on Linux). Not guarded: which
  element is truncated isn't visible from the spec, so check `className` in
  `gui/src` before asserting on its text.
- **When an e2e spec needs Shift+key, dispatch a `KeyboardEvent` with
  `shiftKey` instead of `browser.keys([Key.Shift, …])`.** Why: WebKitGTK's
  driver drops the Shift, so Shift+Tab arrived as Tab. Enforced by
  `gui/scripts/guard-e2e-webkit.mjs`.
- **When an e2e spec needs an element's size or position, use `rectOf()`
  from `gui/e2e/helpers.ts` (`getBoundingClientRect` via `browser.execute`),
  never `getSize()`/`getLocation()`.** Why: WebKitGTK's driver answers the
  element-rect command with "element not interactable" for elements like
  table rows (power.e2e.ts's row-height check failed only on Linux).
  Enforced by `gui/scripts/guard-e2e-webkit.mjs`.

## Comments & docs
- Doc-comment (`///`) the public engine API: it's the contract and renders in
  `cargo doc`. One or two lines — say *what* and *why*, not *how*.
- Inline comments earn their place only for non-obvious *why* (e.g. the chunk
  loop's pause/throttle hook points). Don't narrate what the code already says.
- Prefer clear names over comments.
- **A user-facing change updates its page under `docs/`: CLI flags →
  `docs/cli.md`, GUI behaviour → `docs/gui.md`, engine behaviour →
  `docs/how-it-works.md`. A headline feature also updates the README
  highlights; the README stays a short landing page, and detail goes in
  `docs/`. When the main view changes visibly, regenerate the screenshots
  (`pnpm screenshots` in `gui/e2e`).** Why: the README is the first
  impression and goes stale quietly.
- **The README and `docs/` never name milestones (`M5`, `M6`…) or link
  milestone tickets; for status or plans, link `roadmap.md`.** Why:
  milestone numbers mean nothing to a newcomer and go stale as each one
  ships.

## Definition of Done (every change & milestone)
Standing gates — established in M0, apply to all work, and deliberately NOT
repeated in each milestone's acceptance criteria:
- `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` pass.
- CI is green. PR CI is Linux-only (Windows runs on `main`), so run the
  gate locally on Windows before each push. Why: Windows Actions minutes bill
  at 2x.
- New/changed behaviour ships with a runnable test; the engine/CLI split holds.
- User-facing changes update the README and `docs/` (Comments & docs).

## Milestones
Roadmap index: `roadmap.md`. Each milestone is a ticket in `milestones/`
(`milestone-NN-*.md`) with runnable acceptance criteria. Template:
`milestones/_template.md`.
