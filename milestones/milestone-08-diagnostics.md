---
id: M8
title: Diagnostics — finding the bug when rdm crashes
status: done
depends-on: [M7]
tags: [engine, cli, gui, testing]
---

## Goal
When a user's rdm crashes or misbehaves, the log on their disk says why, and
they can hand it over in two clicks without leaking their download URLs.

## Context / rationale
rdm now ships prebuilt releases, so bugs show up on machines the developer
can't reach. Today a crash usually leaves nothing to go on.

**What exists today** (checked against the code at v0.2.0):
- **GUI file log.** `init_logging` / `install_logging` in
  `gui/src-tauri/src/lib.rs` write a daily rolling log,
  `rdm.YYYY-MM-DD.log` (UTC dates, last 7 kept), into the **state
  directory** `~/.rdm` (or `$RDM_HOME`), next to `state.json`. It's not the
  OS app-data directory. Level `info` by default, overridable with
  `RUST_LOG`. It goes through `tracing_appender::non_blocking` with default
  settings, and the `WorkerGuard` is leaked with `Box::leak`, so it's never
  dropped and never flushes on exit. A copy also goes to stderr, which only
  `tauri dev` shows: the release Windows build uses
  `windows_subsystem = "windows"` and has no console.
- **CLI.** Logs to stderr only: `warn` by default, `info` with `-v`,
  `RUST_LOG` overrides. The engine's own ERROR events are hidden because the
  CLI prints the returned error itself. It has no log file.
- **Per-download Log tab.** The Manager's `LogLayer` keeps each download's
  recent lines in memory: up to 200 info-and-up lines plus 300 debug lines.
  The tab's **Copy** puts the *visible* lines on the clipboard (debug lines
  only with **Detailed** on). Lines from before the buffer cap, from other
  downloads, and app-wide events aren't reachable from the UI.
- **Redaction.** `rdm::redact_url` (`src/engine/redact.rs`) drops
  `user:pass@` and the fragment and replaces a query string with
  `?<redacted>`. The engine's `download` span logs the URL through it, and
  the Log tab records only event fields, never the span's URL. `state.json`
  keeps full URLs.
- **Startup failures** (for example a list saved by a newer rdm) already get
  a native error dialog (`report_startup_error`).

**Gaps:**
1. **No panic hook** in the CLI or the GUI. In the release GUI the default
   hook prints to a stderr nobody sees, so the log just stops.
2. **Buffered lines can be lost.** The non-blocking writer hands lines to a
   worker thread. Whatever is still in its channel when the process dies is
   lost, and the leaked guard means even a clean exit never waits for the
   flush. The default is also *lossy*: under a burst that fills the channel,
   lines are dropped rather than written.
3. **Frontend errors never reach the log.** React render errors,
   `window.onerror` and `unhandledrejection` stay in the webview console. There's no
   error boundary, so a render error unmounts the root (React 19) and the
   window goes blank.
4. **No startup line** with the app version, OS, WebView version and the
   settings that matter, so a log can't be tied to a build.
5. **No way to find logs in the app:** there's no "Open logs folder" and no
   "Copy diagnostics".
6. **No bug-report issue template.** `.github/` has no `ISSUE_TEMPLATE`.
7. **A panic in a download task goes unnoticed.** `spawn_task`
   (`src/engine/manager/task.rs`) runs `download(...)` then `inner.finish(id,
   result)` in one spawned future. If anything in it panics, including a
   segment worker (they're polled in the same task through
   `FuturesUnordered`), the task unwinds and `finish` never runs. The
   `JoinHandle` is kept but never awaited, only aborted, so nothing sees
   the `JoinError`. The record stays `Downloading` and `task` stays `Some`,
   so it keeps holding a queue slot until restart. A panic while holding
   the Manager's `std::sync::Mutex` (for example inside `on_progress` or
   `claim_target`) would poison it. Every later `lock().unwrap()` then
   panics, and every Tauri command breaks.
8. **No way to copy all logs.** The Log tab copies one download's visible
   lines.
9. **A panicking command kills or hangs the app.** Checked with a
   fault-injection build (WebView2): a synchronous command's panic unwinds
   into the webview's IPC callback, which can't unwind, so the process
   aborts ("panic in a function that cannot unwind"). An async command's
   panic kills its task and the webview's `invoke` never settles.

The engine/frontend split holds (see [AGENTS.md](../AGENTS.md)): panic
containment for download tasks is engine work. Panic hooks, the file log and
the diagnostics report belong to each frontend's process setup. The GUI
only renders and calls commands.

Roadmap: [roadmap.md](../roadmap.md).

## Scope
**In:**
- **Panic hook (GUI and CLI).** It's chained before the default hook, so dev
  stderr output stays, and it logs every panic on every thread. It logs at
  ERROR through `tracing` with the message, `file:line:col`, the thread name
  and the app version; the GUI's record also has a backtrace
  (`std::backtrace::Backtrace::force_capture`), the CLI logs it at DEBUG.
  The CLI's hook also prints a one-line "this is a bug, please report it"
  hint with the version and the issues URL.
- **Logs reach disk.** The decision, so not every line pays for a sync
  write:
  - Keep the non-blocking writer for normal lines. Its worker writes each
    line to the file (OS page cache) as soon as it takes it from the
    channel, so a process crash only loses what's still queued, not what
    the OS holds.
  - Build it with `NonBlockingBuilder::lossy(false)`, so a burst blocks
    briefly instead of dropping lines.
  - Keep the `WorkerGuard` in a process-wide slot (`OnceLock<Mutex<Option<WorkerGuard>>>`)
    instead of leaking it. On Tauri's `RunEvent::Exit`, `flush_logs()`
    waits for the queue to drain, drops the guard and syncs the file.
  - The panic hook waits (at most 1 s) for the lines queued before the
    panic to be written, so they land in order, then appends the panic
    record to today's log file **synchronously** with its own
    `OpenOptions::append` handle and calls `sync_all`. It does **not** drop
    the guard: download-task and command panics are contained, so the app
    keeps running and must keep logging. The record lands even if the
    subscriber is wedged and the process aborts right after.
  - The startup lines go through the same synchronous append path as the
    panic record, so a crash right after launch still leaves the version on
    disk.
  - **Rate-limited sync** (decision 1): after the worker writes a WARN or
    ERROR line it syncs the file, at most once every 5 s; a sync held back
    by the limit happens with the next line after the interval, and the
    exit flush covers a quiet tail. No per-line fsync.
- **Download-task panics are contained (engine).** `spawn_task` wraps the
  download in `FutureExt::catch_unwind` (`AssertUnwindSafe`). A caught
  panic becomes a new `Error::Internal(String)` carrying the panic message.
  `finish` marks the download **Failed** with "internal error, please
  report it: <panic message>", logs it at ERROR under the task span (so it shows in the Log
  tab too), frees the slot and starts the next queued download. Manager
  locking tolerates poison: `lock()` goes through one helper that recovers
  with `PoisonError::into_inner` and logs a warning once, instead of
  `unwrap()`.
- **Contained command panics (GUI, decision 5).** One wrapper around the
  generated invoke handler catches a synchronous command's panic and rejects
  that `invoke` with "rdm hit an internal error running <command>, a bug.
  Details are in the log."; async commands run their body through a shared
  `contained()` helper that does the same. The frontend shows the rejection
  like any failed action.
- **Startup line (GUI).** One INFO line at startup:
  `rdm <version> (<git sha if available>) · <OS name/version/arch> · WebView <tauri::webview_version()> · log level <filter> · memory <RSS>`.
  The OS version and the process's resident memory come from a small
  `rdm::platform` module (no new crates) and also go into Copy
  diagnostics.
  A second line lists the settings that matter: connections, queue cap,
  speed limit, data dir overridden yes/no, and the number of downloads by
  status. No URLs, paths or tags. The CLI logs the same version and OS line
  at `info` (`-v`).
- **Frontend errors reach the log (GUI).**
  - A top-level React error boundary shows a fallback screen (decision 6):
    "Something went wrong", a plain sentence, **Reload**, **Open logs
    folder** and **Copy diagnostics** buttons, and the error message behind
    a collapsed **Show details**.
  - `window.onerror` and `unhandledrejection` handlers.
  - All three call a new `log_frontend_error` Tauri command (message,
    stack, component stack, source), which logs at ERROR with target
    `rdm::frontend`. Rate-limited in the frontend, at most 20 reports per
    minute, so an error in a render loop can't flood the log.
- **"Open logs folder" (GUI).** A Tauri command that opens the log
  directory through the opener plugin. It reuses `open_download`'s
  `RDM_OPEN_LOG` test seam, so e2e can check it without launching a file
  manager. It's in the settings menu and on the fallback screen.
- **"Copy diagnostics" (GUI).** A `diagnostics()` Tauri command that
  returns plain text and the frontend puts on the clipboard. It has:
  - the version, OS and WebView lines;
  - the settings that matter;
  - the log directory;
  - the last 300 lines of the newest log file (decision 3) (falling back to the previous
    day's file when today's is short).

  Every URL in the text goes through a new engine helper
  `redact_urls_in(text)`, which finds URL-shaped tokens and applies
  `redact_url` to each. Defence in depth: the file log should already be
  redacted, but lines from `RUST_LOG=debug`, dependencies or frontend error
  messages might not be. It's in the settings menu and on the fallback
  screen.
- **Bug-report issue template.** `.github/ISSUE_TEMPLATE/bug_report.yml`
  (a GitHub form) asks for:
  - the rdm version, OS, and CLI or GUI;
  - what happened and what was expected;
  - steps to reproduce;
  - a "Diagnostics" textarea. Its help text says where Copy diagnostics is
    and, for the CLI, to rerun with `-v` (or `RUST_LOG=debug`) and paste
    the output, checking it for anything private first.

  Plus `config.yml` pointing security reports to SECURITY.md.
- **Docs.** A **Logs and troubleshooting** section in `docs/gui.md`
  (replacing "State and logs"' log paragraph), `docs/cli.md` and
  `docs/how-it-works.md`. It covers:
  - where the logs are;
  - what's redacted;
  - Copy diagnostics and Open logs folder;
  - what happens on a panic: a download task panic fails that download,
    while an app panic is logged and the app exits;
  - how to file a bug.

  Fix `docs/gui.md`'s claim that logs hold full download URLs: logged URLs
  are redacted, and only `state.json` keeps them in full. `SECURITY.md`'s
  "There are no releases yet" line is stale; update it. README:
  one line linking the troubleshooting section, if it fits the landing page.

**Out (deferred):**
- Automatic crash upload or telemetry (Sentry or similar). The owner
  doesn't want user tracking or telemetry: everything stays local and
  redacted (→ backlog: "Crash
  reporting/telemetry (opt-in)").
- Native minidumps for crashes that aren't Rust panics (access violations
  in WebView2 or native code) (→ backlog with the above).
- A CLI log file. The CLI runs in a terminal, so stderr plus `-v` and
  `RUST_LOG` is enough. Revisit if the CLI gains a daemon or queue mode.
- An in-app log viewer for the app-wide log.
- Code signing (backlog) and the in-app updater (M9).

## Tasks
### PR A — crash containment and durable logging (engine, CLI, GUI crate)
- [x] Engine: `Error::Internal(String)`; `spawn_task` catches a panic with
  `catch_unwind` and finishes the download as Failed with an ERROR line.
  Doc comments on the new public items.
- [x] Engine: a poison-tolerant lock helper used for every Manager `Mutex`
  (and every other engine lock: the log buffers, rate limiters and tallies
  outlive a task too).
- [x] Engine: a fault-injection seam for tests. It's a cargo feature
  `fault-injection`, off by default and never enabled by the release
  builds (dist or tauri-action): it refuses to compile without debug
  assertions. It panics inside the download task when a
  request URL's path is `/__panic`, so `tests/` can drive it against the
  fixture server. CI runs the integration test with the feature on.
- [x] GUI crate: move logging setup out of `lib.rs` into a `logging`
  module that doesn't need a Tauri app:
  - `install_logging` returns nothing and keeps the guard in the
    process-wide slot;
  - `lossy(false)`;
  - `flush_logs()` drains the queue and drops the guard;
  - `install_panic_hook()` chains the default hook and writes the
    synchronous panic record (the log dir comes from `install_logging`, so
    the hook is installed first thing in `run()`);
  - the rate-limited sync from decision 1.
- [x] GUI crate: call `flush_logs()` on `RunEvent::Exit`.
- [x] GUI crate: startup version, OS, WebView, memory and settings lines.
- [x] GUI crate: the child-process tests re-run their own test binary with
  an env var (`RDM_LOG_TEST=panic|burst`) and panic in the test itself, so
  the app needs no panic trigger of its own.
- [x] GUI crate: contain command panics (decision 5), with the
  `fault-injection` feature's `test_panic` / `test_panic_async` commands
  for `gui/e2e/specs/command-panic.e2e.ts`. The e2e build turns the
  feature on.
- [x] CLI: panic hook (ERROR event plus the "please report" hint with the
  version) and the `-v` version/OS line.
- [x] Docs: `docs/how-it-works.md` (panic containment, log durability) and
  `docs/cli.md` (troubleshooting, bug reports).

### PR B — frontend errors, diagnostics UI, issue template
- [x] Tauri commands `log_frontend_error`, `open_logs_folder` and
  `diagnostics`, mirrored in `gui/src/lib/api.ts`. Also `logs_folder` (the
  button's tooltip), `save_text_file` (Save…, with an `RDM_SAVE_PATH` seam)
  and `open_bug_report` (the fallback's GitHub link: a fixed URL, so the
  webview can't open one of its choosing). The diagnostics report writes
  the home folder as `~` throughout, log lines included.
- [x] Engine: `redact_urls_in(text)` next to `redact_url`, with a doc
  comment and unit tests.
- [x] GUI: an error boundary with a fallback screen, global `error` and
  `unhandledrejection` handlers, and a rate-limited reporter
  (`gui/src/lib/errorReport.ts`).
- [x] GUI: Open logs folder and Copy diagnostics in the settings menu and
  on the fallback screen, with a "Copied" confirmation like the Log tab's.
- [x] Log tab (decision 2): the existing **Copy** copies every buffered
  line for the download, whatever Detailed says, and **Save…** saves the
  same lines as plain text through a save dialog (default name
  `<download name>.log`), with a test seam so e2e opens no real dialog.
  A **Simple | Detailed** segmented switch on the left replaces the
  Detailed checkbox; the actions sit on the right.
- [x] Move `stubClipboard` / `restoreClipboard` from `log.e2e.ts` into
  `gui/e2e/helpers.ts` for reuse.
- [x] Troubleshooting group (decision 4): visible to everyone (no
  developer mode), quiet and secondary: last in the settings menu, muted.
- [x] `.github/ISSUE_TEMPLATE/bug_report.yml` and `config.yml`.
- [x] Docs: `docs/gui.md` "Logs and troubleshooting" (and the URL wording
  fix), the `SECURITY.md` supported-versions line, and the README link if
  it fits.

## Acceptance criteria
_Standing gates (fmt/clippy/test/CI) live in AGENTS.md's Definition of Done
and apply here without being restated. E2E specs follow AGENTS.md's WebKitGTK
rules (`waitForGone`, `waitForText` for truncated text, no repeated
characters through `browser.keys`, Shift via a dispatched `KeyboardEvent`,
`rectOf` for geometry). Ordering assertions use Manager status, not progress
bytes._

**Engine (`cargo test --features fault-injection` for the panic cases):**
- [x] A download whose task panics (`/__panic` fixture) ends **Failed** with
  an error naming an internal error. The panic message is in that
  download's log lines at ERROR. A second download that was queued behind
  it (queue cap 1) then starts and completes, asserted on Manager status.
  (`tests/panic.rs`)
- [x] After that panic, `list`, `add`, `pause` and `remove` still work: no
  poisoned-lock panic. (`tests/panic.rs`; a poisoned state lock itself in
  `manager::tests::commands_survive_a_poisoned_state_lock`)
- [x] `redact_urls_in` redacts every URL in a multi-line text containing
  `https://user:pw@host/p?token=SECRET#frag`, a bare `http://h/x?sig=abc`
  and a URL inside quotes and parentheses. No `SECRET`, `sig=abc`, `pw` or
  `#frag` survives, and text without URLs is unchanged. A login shows as
  `<redacted>:<redacted>@` (`<redacted>@` for a user name alone), and
  redacting twice changes nothing (decision 7).
  (`src/engine/redact.rs` unit tests)
- [x] A panic on the blocking pool (hashing a file named `__panic…`)
  fails that step as an internal error: a Manager checksum check ends
  (state None) instead of staying pending (`tests/panic.rs`), and the CLI exits 101 with
  "internal error, please report it" (`tests/cli_panic.rs`).
- [x] A default build (`cargo build`, no features) has no `/__panic`
  behaviour: the same fixture request fails as a normal 404.

**GUI crate (`cargo test -p rdm-gui`):**
- [x] **Panic reaches disk.** A test spawns its own test binary as a child
  process with `RDM_LOG_TEST=panic` and a temp log dir. The child
  installs logging and the panic hook, logs the startup line, then panics.
  After the child exits with a non-zero status, the parent reads the log
  file and asserts it contains:
  - the version line (`rdm 0.x.y`);
  - the panic message;
  - its `file:line`;
  - and that the panic record comes after the startup line.
- [x] **Burst isn't lossy.** A child process logs 200,000 lines and then
  exits through `flush_logs()`. The file holds all 200,000.
- [x] **Sync is rate-limited.** The first WARN/ERROR line syncs at once;
  more within 5 s wait; the next line after the interval syncs them; INFO
  lines alone never do. (`logging::tests::syncs_are_rate_limited`)
- [x] **Command panics are contained.** In the e2e build, invoking
  `test_panic` (sync) and `test_panic_async` rejects with the internal
  error text, and `list` still answers afterwards.
  (`gui/e2e/specs/command-panic.e2e.ts`)
- [x] **Diagnostics redact.** `diagnostics()` over a temp log dir whose log
  contains a signed URL and a `user:pass@` URL returns text with neither
  secret. It also contains the version line and the last line of the log.
- [x] `log_frontend_error` writes one ERROR line with target
  `rdm::frontend`, the message and the stack.

**CLI (`cargo test --features fault-injection --test cli_panic`):**
- [x] With the `fault-injection` feature, `rdm <fixture>/__panic` exits
  non-zero. Stderr contains the panic message, the version and the issues
  URL hint. (`tests/cli_panic.rs`, `assert_cmd`-style)

**Frontend (Vitest):**
- [x] A component that throws during render, wrapped in the error boundary,
  shows the fallback screen (heading, Reload, Open logs folder, Copy
  diagnostics, and the message once Show details is opened). The mocked `log_frontend_error` is invoked once with
  the message and a component stack.
- [x] Dispatching an `ErrorEvent` and a `PromiseRejectionEvent` on `window`
  each invoke `log_frontend_error`. 50 errors in a burst invoke it at most
  20 times.
- [x] Log tab **Copy** copies every buffered line, including debug ones,
  with Detailed off; the save action saves the same text.

**E2E (`gui/e2e/specs/diagnostics.e2e.ts`, against the fixture server):**
- [x] **Open logs folder:** clicking it in the settings menu appends
  `open <RDM_HOME>` (the log dir) to `RDM_OPEN_LOG`, checked with
  `openLog()`.
- [x] **Copy diagnostics:**
  - Add a fixture URL with `?token=SECRET`, then click Copy diagnostics
    with the clipboard stubbed.
  - The text has the version line and `?<redacted>`, and doesn't have
    `SECRET`.
  - The button's "Copied" confirmation is read with `waitForText`.
- [x] **Fallback screen:** a fault-injection-only command (or a
  `browser.execute` throw inside a render through a test hook) shows the
  fallback screen. Its Copy diagnostics works, and the log file contains
  the `rdm::frontend` ERROR line.

**Repo:**
- [x] `.github/ISSUE_TEMPLATE/bug_report.yml` is valid YAML with required
  version, OS and steps fields (checked by a small script in `pnpm lint`,
  or by `yq` in CI).

## Decisions (owner's answers to the open questions)
1. **ERROR lines and disk sync:** flushed to disk "occasionally": the
   worker syncs after a WARN/ERROR line at most once every 5 s, never per
   line. The panic record and the exit flush stay synchronous.
2. **Copying a download's lines:** the Log tab has **one** copy action, the
   existing **Copy**, which now copies every buffered line for the download
   (up to 200 info-and-up plus 300 debug), whatever the Detailed toggle
   says; there's no separate "Copy all". A save action writes the same lines
   to a plain-text file through a save dialog, default name
   `<download name>.log`. Approved mock-up: the save button is labelled
   **Save…** with the tooltip "Save the log as a file" ("Export log…" was
   rejected). The toolbar puts the view control on the left, a segmented
   **Simple | Detailed** switch that replaces the Detailed checkbox (same
   stored preference, exposing its pressed state), and the actions,
   **Copy** and **Save…**, on the right. PR B, with e2e coverage.
3. **Copy diagnostics log tail:** 300 lines.
4. **Placement:** a "Troubleshooting" group in the settings menu, plus the
   fallback screen. It is always visible, not behind a developer mode, so
   someone filing a bug needn't find a hidden switch first; it sits last
   in the menu and is styled quiet and secondary: a small muted heading,
   ghost buttons **Open logs folder** (its tooltip is the folder's path)
   and **Copy diagnostics**, and the hint "Links in diagnostics have
   passwords and query strings removed." (The mock-up's "Diagnostics
   contain no download links." wasn't true, since redacted links keep
   their host and path; see decision 7.)
5. **Crashes outside downloads:** any crash is logged: the panic hook logs
   every panic on every thread, and the locks are poison-tolerant. A
   command that panics is contained (gap 9 shows Tauri doesn't): the
   `invoke` rejects with an internal error the frontend shows, and the app
   keeps working. The startup line and Copy diagnostics include the
   process's resident memory. No user tracking or telemetry: everything
   stays local and redacted.
6. **Fallback screen** (approved mock-up): centred, an alert icon above
   the heading "Something went wrong", the plain sentence "rdm hit a
   problem drawing this window. Your downloads keep running.", then
   **Reload**, **Open logs folder** and **Copy diagnostics**, and a line
   saying to paste the diagnostics into a new GitHub issue. The technical
   error text (e.g. `TypeError: …`) sits behind a **Show details**
   disclosure, collapsed by default and small and muted when open; Copy
   diagnostics still carries the full error and component stack.
7. **What redaction shows:** a redacted link keeps its scheme, host and
   path, and the wording says so (the hint, `docs/`, the issue template).
   A login is replaced, not dropped, so a report still shows one was used
   (useful for a 401 or 403): `<redacted>:<redacted>@` with a password,
   `<redacted>@` for a user name alone, e.g.
   `https://user:pw@example.com/files/report.pdf?token=abc#f` →
   `https://<redacted>:<redacted>@example.com/files/report.pdf?<redacted>`.
   This is in `redact_url` itself, so the file log, the Log tab, the CLI's
   errors and diagnostics all agree.
