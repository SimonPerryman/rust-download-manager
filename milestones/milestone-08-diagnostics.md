---
id: M8
title: Diagnostics — finding the bug when rdm crashes
status: todo
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
   (`src/engine/manager.rs`) runs `download(...)` then `inner.finish(id,
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

The engine/frontend split holds (see [AGENTS.md](../AGENTS.md)): panic
containment for download tasks is engine work. Panic hooks, the file log and
the diagnostics report belong to each frontend's process setup. The GUI
only renders and calls commands.

Roadmap: [roadmap.md](../roadmap.md).

## Scope
**In:**
- **Panic hook (GUI and CLI).** It's chained before the default hook, so dev
  stderr output stays. It logs at ERROR through `tracing` with the message,
  `file:line:col`, the thread name, the app version and a backtrace
  (`std::backtrace::Backtrace::force_capture`). In the GUI it then flushes
  the file log (see below) before returning. The CLI's hook also prints a
  one-line "this is a bug, please report it" hint with the version and the
  issues URL.
- **Logs reach disk.** The decision, so not every line pays for a sync
  write:
  - Keep the non-blocking writer for normal lines. Its worker writes each
    line to the file (OS page cache) as soon as it takes it from the
    channel, so a process crash only loses what's still queued, not what
    the OS holds.
  - Build it with `NonBlockingBuilder::lossy(false)`, so a burst blocks
    briefly instead of dropping lines.
  - Keep the `WorkerGuard` in a process-wide slot (`OnceLock<Mutex<Option<WorkerGuard>>>`)
    instead of leaking it. Take it and drop it (which flushes and joins the
    worker) on Tauri's `RunEvent::Exit` and in the panic hook.
  - The panic hook first drops the guard, so queued lines land in order,
    then appends the panic record to today's log file **synchronously**
    with its own `OpenOptions::append` handle and calls `sync_all`. The
    record lands even if the subscriber is wedged and the process aborts
    right after.
  - The startup lines go through the same synchronous append path as the
    panic record, so a crash right after launch still leaves the version on
    disk. ERROR lines aren't synced per line (Open question 1).
- **Download-task panics are contained (engine).** `spawn_task` wraps the
  download in `FutureExt::catch_unwind` (`AssertUnwindSafe`). A caught
  panic becomes a new `Error::Internal(String)` carrying the panic message.
  `finish` marks the download **Failed** with "Internal error — please
  report it", logs it at ERROR under the task span (so it shows in the Log
  tab too), frees the slot and starts the next queued download. Manager
  locking tolerates poison: `lock()` goes through one helper that recovers
  with `PoisonError::into_inner` and logs a warning once, instead of
  `unwrap()`.
- **Startup line (GUI).** One INFO line at startup:
  `rdm <version> (<git sha if available>) · <OS name/version/arch> · WebView <tauri::webview_version()> · log level <filter>`.
  A second line lists the settings that matter: connections, queue cap,
  speed limit, data dir overridden yes/no, and the number of downloads by
  status. No URLs, paths or tags. The CLI logs the same version and OS line
  at `info` (`-v`).
- **Frontend errors reach the log (GUI).**
  - A top-level React error boundary shows a fallback screen: "Something
    went wrong", the error message, and **Reload**, **Open logs folder**
    and **Copy diagnostics** buttons.
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
  - the last 300 lines of the newest log file (falling back to the previous
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
- Automatic crash upload or telemetry (Sentry or similar). It needs a
  server, a privacy policy and opt-in consent (→ backlog: "Crash
  reporting/telemetry (opt-in)").
- Native minidumps for crashes that aren't Rust panics (access violations
  in WebView2 or native code) (→ backlog with the above).
- A CLI log file. The CLI runs in a terminal, so stderr plus `-v` and
  `RUST_LOG` is enough. Revisit if the CLI gains a daemon or queue mode.
- An in-app log viewer for the app-wide log.
- Code signing and the in-app updater (already in the backlog).

## Tasks
### PR A — crash containment and durable logging (engine, CLI, GUI crate)
- [ ] Engine: `Error::Internal(String)`; `spawn_task` catches a panic with
  `catch_unwind` and finishes the download as Failed with an ERROR line.
  Doc comments on the new public items.
- [ ] Engine: a poison-tolerant lock helper used for every Manager `Mutex`.
- [ ] Engine: a fault-injection seam for tests. It's a cargo feature
  `fault-injection`, off by default and never enabled by the release
  builds (dist or tauri-action). It panics inside the download task when a
  request URL's path is `/__panic`, so `tests/` can drive it against the
  fixture server. CI runs the integration test with the feature on.
- [ ] GUI crate: move logging setup out of `lib.rs` into a `logging`
  module that doesn't need a Tauri app:
  - `install_logging` returns nothing and keeps the guard in the
    process-wide slot;
  - `lossy(false)`;
  - `flush_logs()` drops the guard;
  - `install_panic_hook(dir)` chains the default hook and writes the
    synchronous panic record.
- [ ] GUI crate: call `flush_logs()` on `RunEvent::Exit` (switch `run()` to
  `.build()?.run(|_, event| …)`).
- [ ] GUI crate: startup version, OS, WebView and settings lines.
- [ ] GUI crate: a test-only panic trigger, the `fault-injection` feature
  with env `RDM_TEST_PANIC=startup`, for the child-process test below.
- [ ] CLI: panic hook (ERROR event plus the "please report" hint with the
  version) and the `-v` version/OS line.
- [ ] Docs: `docs/how-it-works.md` (panic containment, log durability) and
  `docs/cli.md` (troubleshooting, bug reports).

### PR B — frontend errors, diagnostics UI, issue template
- [ ] Tauri commands `log_frontend_error`, `open_logs_folder` and
  `diagnostics`, mirrored in `gui/src/lib/api.ts`.
- [ ] Engine: `redact_urls_in(text)` next to `redact_url`, with a doc
  comment and unit tests.
- [ ] GUI: an error boundary with a fallback screen, global `error` and
  `unhandledrejection` handlers, and a rate-limited reporter
  (`gui/src/lib/errorReport.ts`).
- [ ] GUI: Open logs folder and Copy diagnostics in the settings menu and
  on the fallback screen, with a "Copied" confirmation like the Log tab's.
- [ ] Log tab: see Open question 2 (recommended: **Copy all**).
- [ ] Move `stubClipboard` / `restoreClipboard` from `log.e2e.ts` into
  `gui/e2e/helpers.ts` for reuse.
- [ ] `.github/ISSUE_TEMPLATE/bug_report.yml` and `config.yml`.
- [ ] Docs: `docs/gui.md` "Logs and troubleshooting" (and the URL wording
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
- [ ] A download whose task panics (`/__panic` fixture) ends **Failed** with
  an error naming an internal error. The panic message is in that
  download's log lines at ERROR. A second download that was queued behind
  it (queue cap 1) then starts and completes, asserted on Manager status.
  (`tests/panic.rs`)
- [ ] After that panic, `list`, `add`, `pause` and `remove` still work: no
  poisoned-lock panic. (`tests/panic.rs`)
- [ ] `redact_urls_in` redacts every URL in a multi-line text containing
  `https://user:pw@host/p?token=SECRET#frag`, a bare `http://h/x?sig=abc`
  and a URL inside quotes and parentheses. No `SECRET`, `sig=abc`, `pw` or
  `#frag` survives, and text without URLs is unchanged.
  (`src/engine/redact.rs` unit tests)
- [ ] A default build (`cargo build`, no features) has no `/__panic`
  behaviour: the same fixture request fails as a normal 404.

**GUI crate (`cargo test -p rdm-gui`):**
- [ ] **Panic reaches disk.** A test spawns its own test binary as a child
  process with `RDM_TEST_PANIC=startup` and a temp log dir. The child
  installs logging and the panic hook, logs the startup line, then panics.
  After the child exits with a non-zero status, the parent reads the log
  file and asserts it contains:
  - the version line (`rdm 0.x.y`);
  - the panic message;
  - its `file:line`;
  - and that the panic record comes after the startup line.
- [ ] **Burst isn't lossy.** A child process logs 200,000 lines and then
  exits through `flush_logs()`. The file holds all 200,000.
- [ ] **Diagnostics redact.** `diagnostics()` over a temp log dir whose log
  contains a signed URL and a `user:pass@` URL returns text with neither
  secret. It also contains the version line and the last line of the log.
- [ ] `log_frontend_error` writes one ERROR line with target
  `rdm::frontend`, the message and the stack.

**CLI (`cargo test`):**
- [ ] With the `fault-injection` feature, `rdm <fixture>/__panic` exits
  non-zero. Stderr contains the panic message, the version and the issues
  URL hint. (`tests/cli_panic.rs`, `assert_cmd`-style)

**Frontend (Vitest):**
- [ ] A component that throws during render, wrapped in the error boundary,
  shows the fallback screen (heading, message, Reload, Open logs folder,
  Copy diagnostics). The mocked `log_frontend_error` is invoked once with
  the message and a component stack.
- [ ] Dispatching an `ErrorEvent` and a `PromiseRejectionEvent` on `window`
  each invoke `log_frontend_error`. 50 errors in a burst invoke it at most
  20 times.
- [ ] Log tab **Copy all** (if adopted) copies every buffered line,
  including debug ones, with Detailed off.

**E2E (`gui/e2e/specs/diagnostics.e2e.ts`, against the fixture server):**
- [ ] **Open logs folder:** clicking it in the settings menu appends
  `open <RDM_HOME>` (the log dir) to `RDM_OPEN_LOG`, checked with
  `openLog()`.
- [ ] **Copy diagnostics:**
  - Add a fixture URL with `?token=SECRET`, then click Copy diagnostics
    with the clipboard stubbed.
  - The text has the version line and `?<redacted>`, and doesn't have
    `SECRET`.
  - The button's "Copied" confirmation is read with `waitForText`.
- [ ] **Fallback screen:** a fault-injection-only command (or a
  `browser.execute` throw inside a render through a test hook) shows the
  fallback screen. Its Copy diagnostics works, and the log file contains
  the `rdm::frontend` ERROR line.

**Repo:**
- [ ] `.github/ISSUE_TEMPLATE/bug_report.yml` is valid YAML with required
  version, OS and steps fields (checked by a small script in `pnpm lint`,
  or by `yq` in CI).

## Open questions
1. **Sync ERROR lines too?** Should every ERROR line go to disk with an
   `fsync`, or is the worker thread plus the OS page cache enough?
   **Recommendation: page cache is enough.** A process crash doesn't lose
   what the worker already wrote. Only a power cut or OS crash loses the
   page cache, and per-line fsync on a burst of failing downloads costs
   real I/O. The panic record and the exit flush are the synchronous
   points.
2. **Copying all of a download's lines.** Options:
   - (a) the Log tab gains **Copy all**: every buffered line for that
     download (up to 200 info-and-up plus 300 debug), whatever the
     Detailed toggle says;
   - (b) an **Export log…** that saves that download's lines to a file;
   - (c) nothing beyond Copy diagnostics.

   **Recommendation: (a), in scope for PR B.** It's small: the lines are
   already in the frontend, so it's `copyText(lines)` instead of
   `copyText(shown)`. Copy diagnostics covers the app-wide log, and Open
   logs folder covers the full files, so (b) adds little.
3. **How much log tail goes into Copy diagnostics?** **Recommendation: 300
   lines.** Enough to cover a failed download's start to its error, small
   enough to paste into an issue. The issue template asks for the full file
   only if the developer requests it.
4. **Where do Open logs folder and Copy diagnostics live?**
   **Recommendation: the settings menu,** under a "Troubleshooting" group,
   plus the fallback screen. A Help menu doesn't exist and isn't worth
   adding for two items.
5. **Should the GUI catch panics on Tauri command threads?** A panic in a
   synchronous command handler unwinds into Tauri, which today rejects the
   `invoke` promise or aborts depending on the handler. **Recommendation:**
   rely on the panic hook to log it, and on the poison-tolerant lock so the
   app keeps working. Don't wrap every command; revisit if a real case
   appears.
