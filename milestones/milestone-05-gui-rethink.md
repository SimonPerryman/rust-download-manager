---
id: M5
title: GUI rethink — Console layout
status: done
depends-on: [M2, M3, M4]
tags: [gui, engine, testing]
---

## Goal
Replace the Active/History tab GUI with a Console-style window:
- a sidebar of status and folder filters;
- one sortable table where finished downloads stay where they are and say
  clearly that they're done;
- a details pane that shows each connection's progress.

## Context / rationale
The trigger complaint is that a finished download disappears. It isn't
deleted: it jumps from the Active tab to History, with no "done" moment. An
assessment of the current GUI found more problems:
- Stop deletes the partial file in one click.
- There's no overall status (total speed, the global limit).
- Finished downloads are thin: no checksum result, no time taken.
- History can't be filtered or bulk-cleared.
- Only one URL can be added at a time.
- The look is the stock shadcn kit.

The owner compared three mockup directions and chose Console as the base. A
combined proof of concept takes the best parts of the other two.

The references below live in `.ai-cache/`, which is gitignored and exists
only in the main checkout (`C:\dev\projects\rdm\.ai-cache`). Worktrees read it
there by absolute path; never copy or delete it.

- **Visual and behaviour reference:** `.ai-cache/mockups/m5/d-combined.html`.
  Turn on its "Key" to see every feature with a plain-English description and
  where it came from. The other directions (`a-shelf`, `b-console`, `c-lanes`)
  and the comparison page (`index.html`) are in the same folder.
- **Design rules:** `.ai-cache/mockups/m5/design-plan.md`. It covers
  Refactoring UI principles, the Console palette and type (IBM Plex Sans, with
  Plex Mono only for URLs and hashes), and copy rules. The owner requires
  Refactoring UI.
- **Research brief:** `.ai-cache/m5-ux-research-brief.md`.
- Roadmap: [[roadmap]].

The engine/frontend split holds. Every new behaviour that touches downloads
(stop keeping the partial, reordering, verifying a checksum, clearing,
per-connection progress) is an engine API exposed as a Tauri command. The
frontend does only presentation. That includes grouping by folder, sampling
speed for graphs, and sorting and filtering the snapshot list.

## Scope
**In:**
- **Engine/Tauri:**
  - stop with a choice to keep or delete the partial file;
  - reorder the queue;
  - clear by status;
  - verify a checksum after the download;
  - per-connection progress and checksum state in the snapshot.
- **Window structure:**
  - **Sidebar:** status filters with live counts (All, Downloading, Waiting,
    Paused, Done, Failed, Stopped), folder filters derived from download
    targets, and a total-speed sparkline.
  - **Sortable table** with right-aligned tabular numbers.
  - **Status bar:** total speed, active and waiting counts, and the global
    limit, which can be edited in place.
  - **Toolbar:** filter by name, Pause all / Resume all, Clear finished, Add,
    density, theme.
- **Finish moment:**
  - The row holds a success tint for about 6 s, then keeps a faint tint and a
    solid **Open** button until the file is opened once.
  - An in-app notification offers Open and Show in folder.
  - A finished row never moves or disappears on its own.
- **Quiet done state:** a check, "Done", and a lighter filename. No solid
  green bars.
- **Failed downloads** are pinned to the top of All, with the error inline,
  Retry, and Retry all failed.
- **Lanes:**
  - In the table's Progress cell: one hairline per connection, at most 8 lines
    (neighbours share a line beyond that), converging into one bar on finish.
  - In the details pane: the full-size lanes, with a hover tooltip per lane.
- **Details pane** (resizable):
  - **Overview:** sizes, speed, time left, lanes, URL, folder, checksum state.
    For a finished download it adds a summary line ("5.8 GB in 2 min 14 s,
    average 44 MB/s, checksum verified"), a Verify checksum control, and
    queue up/down buttons for waiting downloads (PR D).
  - **Speed:** a 60 s throughput graph with the limit drawn as a dashed line.
  - **Log:** the download's last 200 engine log lines, polled while the tab
    is open (PR E).
- **Stop asks first.** "Keep the partial file to resume later" is the
  default; "Delete the partial file" is the alternative.
- **Batch add:** one URL per line, with a preview list that flags non-links
  and duplicates and shows a count. Ctrl+V anywhere opens it pre-filled. The
  destination remembers the last folder used.
- **Power features:**
  - right-click menu that fits the row's status;
  - multi-select with Ctrl and Shift, and a contextual toolbar;
  - keyboard: arrows, Home/End, Space to pause or resume, Enter to open, Del
    to remove, Shift+F10 for the menu;
  - drag or Alt+↑/↓ to reorder waiting rows, with "Next up" on the first.
- **Display:**
  - Compact (32 px) and comfortable density.
  - Light and dark themes.
  - Works from a 640 px window: the sidebar becomes an icon rail and
    lower-priority columns hide.

**Out (deferred):**
- The "Key" origin overlay. It's a mockup-only comparison tool and doesn't
  ship.
- Free-space readout in the status bar (open question 1).
- Native OS notifications, tray, and start with the system (backlog).
- Per-type categories and auto-organise (backlog: scheduling and automation).
- A SQLite store, filter-by-query in the engine, and a history retention
  policy (backlog: persistence at scale). History stays until cleared, as
  today.

## Tasks
Split into five PRs. A comes first, because C needs A's snapshot fields. B
can start in parallel on today's snapshot. E (the Log tab) follows C.

### PR A — engine and Tauri commands
- [x] `stop(id, keep_partial)`.
  - Keep: status `cancelled`, the `.part` file and sidecar stay, the snapshot
    gains `partial_kept: bool`, and resume continues from the partial.
  - Delete: today's behaviour.
  - The CLI's Ctrl-C pause is unchanged.
- [x] `move_queued(id, index)`: reorders the waiting set, persisted in the
  store's order; the scheduler starts the first waiting download.
- [x] `clear(statuses)`: removes finished entries in bulk. Files on disk are
  untouched for completed downloads, and partials are removed for stopped ones
  unless they were kept (same rule as `remove`).
- [x] `verify_checksum(id, sha256)`: hashes the completed file off the async
  runtime. The snapshot gains
  `checksum_state: none | pending | verified | mismatch`. The add-time
  checksum feeds the same field.
- [x] Snapshot gains `segments: [{ start, end, done }]`: the download's whole
  segment tiling (finished, unowned and stolen pieces, not just one per live
  connection), updated with progress. Work-stealing shows up as a segment's
  range shrinking plus a new one appended.
- [x] Mirror the new fields in `gui/src/lib/types.ts` and add the commands to
  `gui/src/lib/api.ts`.
- [x] Update the CLI where it matters, for example a `--keep-partial` default
  on cancel if one applies. Keep `README.md` current. (No CLI change: see
  open question 5.)

### PR B — window structure, finish moment, failed downloads
- [x] Replace `Tabs`, `DownloadList` and `DownloadRow` with a sidebar, a table
  and a status bar.
- [x] Status and folder filters with live counts; sortable columns; filter by
  name.
- [x] Finish moment: tint, lingering Open, in-app notification. Track "opened
  once" per download in the GUI's local prefs, since it's presentation state.
- [x] Quiet done state; failed downloads pinned with Retry and Retry all
  failed.
- [x] Status bar with an editable global limit, reusing `RateInput`.
- [x] Clear finished with an undo toast. The undo window delays the
  `clear` call by about 5 s, so nothing is restored after the fact.
- [x] Stop confirm dialog (uses A's `stop`; before A merges, ship without the
  keep option).
- [x] Remove the developer text (`~/.rdm/state.json`) from the UI.
- [x] Restyle to the Console palette and IBM Plex; tokens stay on `:root`
  with dark overrides.

### PR C — details pane and lanes
- [x] Resizable details pane, with the Overview and Speed tabs.
- [x] Lanes component used at two sizes, the table cell and the pane, drawn
  from `segments`.
- [x] Finished summary line and the Verify checksum control. (The summary's
  time comes from a new snapshot field, `active_ms`: time spent running,
  persisted, reset when a download starts again from zero, and unknown
  (`null`) for a run cut short by a crash or an older build's record;
  quitting the app records it first.)
- [x] Speed graph from a frontend ring buffer of `bytes_per_sec` samples, plus
  the sidebar sparkline.
- The Overview's queue up/down buttons for waiting downloads are left to
  PR D, with the other reorder controls (they use `move_queued` too).

### PR E — Log tab
- [x] Engine: a tracing layer that keeps the last ~200 lines per download,
  keyed by the existing `task` span id in `src/engine/manager.rs`. (200
  INFO-and-up lines, in memory only, dropped when the download is removed or
  cleared; its level doesn't follow `RUST_LOG`.)
- [x] A thin `download_log(id)` Tauri command over it.
- [x] A Log tab in the details pane, polled while it's open.
- [x] An engine test for the per-download buffer and an e2e spec for the tab.

### PR D — batch add and power features
- [x] Batch add dialog with preview, Ctrl+V intake for several lines, and the
  remembered destination.
- [x] Context menu, multi-select and the contextual toolbar.
- [x] Keyboard map; waiting-row reorder by drag and Alt+↑/↓ (uses A's
  `move_queued`).
- [x] Density toggle at the new 32/48 px row sizes; narrow-window layout.
- [x] The details pane's Move up / Move down for a waiting download (`move_queued`).

## Acceptance criteria
_Standing gates (fmt/clippy/test/CI) live in AGENTS.md's Definition of Done and
apply here without being restated. E2E specs follow AGENTS.md's WebKitGTK rules
(`waitForGone`, no repeated characters through `browser.keys`)._

**Engine (A):**
- [x] Stopping a download with `keep_partial = true` leaves the `.part` file
  and sidecar on disk, and `partial_kept` is true. Resuming it completes the
  file byte-identical to the fixture, fetching only the remaining ranges.
  (`tests/stop.rs`)
- [x] With `keep_partial = false`, the partial file and sidecar are gone.
  (`tests/stop.rs`)
- [x] With a cap of 1 and three queued downloads, moving the third to index 0
  makes it the next to start when the slot frees. The order survives a
  restart of the store. (`tests/queue.rs`)
- [x] `clear([completed, cancelled])` removes exactly those entries. Completed
  files remain on disk; non-kept partials are deleted. (`tests/clear.rs`)
- [x] `verify_checksum` with the right SHA-256 gives `verified`, and a wrong
  one gives `mismatch`. The snapshot passes through `pending` first.
  (`tests/verify.rs`)
- [x] For a 4-connection download, the snapshot's `segments` cover the file
  without overlap, `done` never decreases within a segment, and the sum of
  `done` equals `downloaded`. (`tests/segments.rs`)

**Engine (E):**
- [x] Each download keeps only its own log lines, at most 200 (oldest dropped
  first), with no URL secrets; they go when it's removed or cleared, and a
  download restored from the store has none. (`tests/download_log.rs`, and
  the bound in `src/engine/download_log.rs`)

**GUI (B–E):** Vitest unit tests for the pure logic (filters, counts,
folder derivation, sorting, URL-list parsing, the lane bucketing rule), plus
these e2e specs against the fixture server:
- [x] **Finish moment:** a download that completes stays in the All view with
  `data-status="completed"`, shows an Open button and a notification, and is
  counted under Done. Opening it once removes the lingering Open state.
  (`specs/finish.e2e.ts`)
- [x] **Filters:** with a downloading, a queued, a completed and a failed
  download, each sidebar filter shows exactly its rows, and the counts match.
  Folder filters split downloads by target folder. (`specs/filters.e2e.ts`)
- [x] **Failed pinned:** a download that fails against the `/seg-403`
  fixture appears at the top of All with its error inline. Retry all failed
  restarts it. (`specs/failed.e2e.ts`)
- [x] **Stop asks first:** Stop opens the dialog. Choosing Keep leaves the
  row Stopped and resumable; choosing Delete leaves it Stopped with Download
  again. (`specs/stop.e2e.ts`)
- [x] **Clear finished:** clears Done and Stopped rows. Undo within the
  window brings them back, and nothing is removed from the engine.
  (`specs/clear.e2e.ts`)
- [x] **Lanes:** a 4-connection download on the `/seg-gated` fixture shows 4
  lanes in its row and in the details pane. Assert on DOM lane count, not
  progress bytes. (`specs/lanes.e2e.ts`; `/gated` has no length or ranges, so
  it can't be multi-connection: `/seg-gated` is `/seg` held by the same gate.)
- [x] **Batch add:** pasting three lines (two URLs and one non-link) previews
  "Add 2 downloads" and adds two rows. (`specs/batch-add.e2e.ts`)
- [x] **Reorder:** with a cap of 1, moving the last waiting row to the top
  with Alt+↑ marks it Next up, and it starts next. Assert on Manager status.
  (`specs/queue.e2e.ts`, extended)
- [x] **Log tab:** a download that fails against `/seg-403` shows its error
  lines in its Log tab; the tab follows the pane to another download, keeps
  polling while open, and its list goes when the tab changes or the pane
  closes. (`specs/log.e2e.ts`)
- [x] Existing specs (`downloads`, `queue`, `ramping`) are updated to the new
  layout and pass.

## Open questions (decide before or during the PR they affect)
1. **Free space in the status bar:** keep it, which needs a `free_space(dir)`
   command, or cut it? It affects B. **Decided (PR B):** cut; moved to the
   backlog.
2. **Log tab:** in M5, which needs the engine to stream per-download log
   lines to the GUI, or later? **Decided:** a separate PR E, after C (see its
   tasks above).
3. **Density default:** compact (Console's default) or comfortable?
   **Decided (PR B):** compact (32 px). PR B ships compact rows only; the
   toggle comes with PR D.
4. **Notification:** in-app only (in scope), or a native OS notification too
   (Tauri notification plugin)? **Decided (PR B):** in-app only; native OS
   notifications stay in the backlog.
5. **Stop CLI parity:** should the CLI's cancel keep the partial by default?
   **Decided (PR A):** the CLI is unchanged. Ctrl-C already pauses and keeps
   the partial; `README.md` says how to discard it.
