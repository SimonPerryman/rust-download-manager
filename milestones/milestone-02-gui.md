---
id: M2
title: GUI
status: todo
depends-on: [M1]
tags: [gui, persistence]
---

## Goal
Make rdm usable without a terminal: a GUI frontend over the existing engine.

## Context / rationale
Chosen as the first post-MVP step (per [[roadmap]]) because it forces a clean
engine API and keeps the MVP lean. The GUI must be a thin frontend — all logic
stays in the engine. Toolkit choice interacts with agentic UI testing: a
web-based shell (Tauri) can be driven by Playwright/expect-style tools; native
toolkits (`egui`, `iced`) cannot, so UI-level automation is limited there.
Because logic lives in the engine (tested at M0/M1), the GUI has little logic
to test regardless.

## Scope
**In:**
- Decide the toolkit (candidates: `egui`, `iced`, Tauri) — first task.
- Add / remove / pause / resume / cancel downloads from the UI.
- Per-download progress, speed, ETA; a download list view.
- Persist the download list + history (**JSON v1 chosen**; swappable store
  boundary, SQLite deferred to the roadmap backlog).
- Keep the CLI a first-class frontend: the GUI is an *additive* second frontend.
  A headless build (no webview/Node) must stay fully functional so `rdm` works
  on a server with no desktop — so the engine + CLI never depend on Tauri.

**Out (deferred):**
- Multi-segment downloads (→ M3).
- Queue management + speed limiting UI (→ M4).

## Tasks
- [x] Toolkit decision recorded (with the agentic-testing trade-off noted) —
      Tauri + React/shadcn/Tailwind; design approved in `.ai-cache/mockups/slick.html`.
- [x] Engine API reviewed/extended for GUI needs (observe + control downloads) —
      engine-side `Manager` + swappable `Store`/`JsonStore` (`src/engine/manager.rs`,
      `src/engine/store.rs`), observed via a `watch` channel of `DownloadSnapshot`s.
- [x] Download list view with live progress/speed/ETA — Active/History tabs,
      per-status rows rendered from the engine's snapshots.
- [x] Add-download flow; pause/resume/cancel controls — Add dialog + per-row
      controls wired to thin Tauri commands over the `Manager`.
- [x] Persistence layer for downloads + history — JSON v1 at `~/.rdm/state.json`
      behind the `Store` boundary; SQLite deferred (roadmap backlog).

## Acceptance criteria
_Standing gates (fmt/clippy/test/CI) live in AGENTS.md's Definition of Done and
apply here without being restated._
- [x] Start, pause, and resume a download entirely from the GUI — the GUI's
      Pause/Resume controls call `Manager::pause`/`resume`; the behaviour is
      asserted deterministically at the engine layer by
      `tests/manager.rs::pause_then_resume_completes` (byte-identical result).
- [x] Downloads + history survive an app restart — asserted by
      `tests/manager.rs::list_and_history_survive_restart` (a new `Manager` over
      the same store restores a completed and a paused download; the paused one
      still resumes). The GUI persists to `~/.rdm/state.json`.
- [x] The GUI contains no download logic — it only calls the engine (verified by
      inspection: every Tauri command is a 1–3 line pass-through to `Manager`;
      `rdm-gui` is a separate workspace crate and the engine/CLI never depend on
      Tauri/Node/webview).

## Implementation notes
- **Observation:** `Manager` exposes `snapshot()` + `subscribe()` (a
  `tokio::sync::watch` of `Vec<DownloadSnapshot>`), with coalesced publishing
  (~10 Hz on progress, immediate on lifecycle changes) so UI update rate is
  decoupled from chunk rate. The Tauri layer re-emits snapshots as a
  `downloads-changed` event.
- **Layout:** Cargo workspace; `rdm` stays at the repo root, the GUI is the
  `rdm-gui` member at `gui/src-tauri`. `default-members = ["."]` keeps the
  default `cargo build`/`cargo test` gate engine/CLI-only and Tauri-free; a new
  CI `gui` job builds the frontend + `rdm-gui` with webview deps.
- **Known limitations (deferred, not blocking):**
  - ~~GUI downloads use a URL-derived filename, not the server's
    `Content-Disposition` name.~~ Resolved in the M2 polish round: the engine
    takes an optional `ClaimTarget` hook on `DownloadRequest`, and the `Manager`
    uses it to claim (and dedup) the server-derived name on the first response,
    showing a URL-derived placeholder until then.
  - No live "Retrying" status: the engine retries transient failures inside a
    single `download()` call, so a retry isn't an observable managed state. The
    mockup's "Retrying" row renders as Failed (retryable via Retry → resume).
  - End-to-end webview/Playwright automation of the GUI is not included; the
    behavioural ACs are proven at the `Manager` layer (above) and the GUI is a
    thin, inspected pass-through. A manual GUI smoke (`pnpm tauri dev` in
    `gui/`) is recommended.
