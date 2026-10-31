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
- Persist the download list + history (SQLite via `sqlx`/`rusqlite`, or JSON v1).

**Out (deferred):**
- Multi-segment downloads (→ M3).
- Queue management + speed limiting UI (→ M4).

## Tasks
- [ ] Toolkit decision recorded (with the agentic-testing trade-off noted).
- [ ] Engine API reviewed/extended for GUI needs (observe + control downloads).
- [ ] Download list view with live progress/speed/ETA.
- [ ] Add-download flow; pause/resume/cancel controls.
- [ ] Persistence layer for downloads + history.

## Acceptance criteria
_Standing gates (fmt/clippy/test/CI) live in AGENTS.md's Definition of Done and
apply here without being restated._
- [ ] Start, pause, and resume a download entirely from the GUI.
- [ ] Downloads + history survive an app restart.
- [ ] The GUI contains no download logic — it only calls the engine
      (verified by inspection; engine tests remain the source of truth).
