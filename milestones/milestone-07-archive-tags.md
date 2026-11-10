---
id: M7
title: Archive & tags
status: done
depends-on: [M5, M6]
tags: [engine, gui, persistence, testing]
---

## Goal
Let users hide finished downloads without deleting them (archive) and label
downloads to find them again (tags).

## Context / rationale
M5 keeps finished downloads in place in one table, so the list only grows.
The only way to shorten it is Clear finished, which removes records for good:
the history of what was downloaded, from where, and its checksum is lost.

- **Archive** hides a download from the working views but keeps its record,
  so the list stays short and nothing is lost. Permanent removal becomes a
  deliberate step in the Archived view.
- **Tags** are user labels ("work", "iso", "course-2") that cut across
  folders and statuses.

The owner agreed this design. Two things are deliberately left out:
- **No per-tag hide switch.** To hide a tag, filter by it, select all, and
  archive. One hiding mechanism is easier to reason about than two.
- **No virtual folders.** The existing folder filters cover real folders,
  and tags cover everything else.

The engine/frontend split holds (see [AGENTS.md](../AGENTS.md)): the archive
flag and tags live on the engine's download record, the operations are
engine API with doc comments exposed as Tauri commands, and the GUI only
filters, counts and renders the snapshot.

**CLI:** gets nothing. The CLI runs one download per invocation and has no
persistent queue or download list (it doesn't use the Manager or
`state.json`), so there's nothing to archive or tag. Revisit if the CLI gains
a queue.

Roadmap: [roadmap.md](../roadmap.md). Builds on
[M5](milestone-05-gui-rethink.md) and comes after
[M6](milestone-06-releases-install.md).

## Scope
**In:**
- **Engine record:** `archived: bool` and `tags: Vec<String>` on
  `DownloadRecord`, mirrored in the snapshot.
- **Store schema v2.** `SCHEMA_VERSION` goes from 1 to 2.
  - A v1 `state.json` loads unchanged: both new fields default (`archived`
    false, `tags` empty) through `#[serde(default)]`, and the next save
    writes `version: 2`.
  - A document with a version newer than the engine knows fails to load
    with a clear error instead of loading and then dropping fields it
    doesn't understand on the next save.
- **Archive:**
  - Only finished downloads (completed, failed, stopped) can be archived.
    Archiving a running or paused one is rejected.
  - Archived rows are excluded from All, the status filters and the folder
    filters, and from their counts.
  - Only finished downloads stay archived: retrying or resuming an archived
    download returns it to the working views. Retry all failed skips
    archived rows.
  - A new sidebar entry, **Archived**, with a live count, lists them. From
    there they can be unarchived (they return to their status and folder
    views) or removed.
  - The toolbar's **Clear finished** becomes **Archive finished** (Done and
    Stopped rows, as Clear finished does today). The undo toast stays: Undo
    unarchives the same ids. Archiving is reversible, so there's no delayed
    call as with clear.
  - **Remove** (permanent) moves to the Archived view: a toolbar action for
    the selection and a context-menu item. It drops the record and **never
    deletes the downloaded file**, nor a partial kept by Stop; any other
    partial (a failed download's `.part` + sidecar) is deleted. Every Remove
    asks first, with a "Don't ask again" box; a Settings toggle turns the
    question back on. Del in the main views archives instead of removing.
- **Tags:**
  - Rules: trimmed, 1 to 32 characters, unique per download ignoring case,
    kept in the order added. At most 16 per download.
  - Add dialog: an optional tag input; the tags apply to every URL added in
    that batch.
  - Editing later: from the row's context menu ("Tags…") and the details
    pane's Overview, for one download or the whole selection.
  - Sidebar **Tags** section: every tag on a non-archived download, with a
    live count; selecting one filters the table, like the folder filters.
    In the Archived view the section follows the view: it lists the tags on
    archived downloads with archived counts, and selecting one narrows the
    Archived view.
  - Add dialog: a pasted URL that matches an archived download is flagged
    with its own note ("In your archive") and is still added, so it can be
    downloaded again.
  - A tag shown on the row as a small label (presentation only).
- **Select all** (Ctrl+A) in the table, if M5 PR D doesn't already ship it,
  so "filter by tag, select all, archive" works.

**Out (deferred):**
- Auto-archive finished downloads after N days (backlog).
- Auto-tag rules by host or file type (backlog).
- Tag rename or merge across all downloads, and tag colours (backlog).
- Search by tag in the name filter.
- Any CLI change (see Context).
- Missing or moved files (backlog): marking a record whose file was deleted
  or moved, "Update file reference" to relink it, and moving files through
  rdm so the record follows them.

## Tasks
### PR A — engine and Tauri commands
- [x] `DownloadRecord` gains `archived` and `tags`; the snapshot carries
  both. Store `SCHEMA_VERSION` = 2; reject newer versions on load.
- [x] `archive(ids) -> usize` and `unarchive(ids) -> usize`: set or clear
  the flag on finished downloads, persist, publish. Return how many changed.
- [x] `archive_finished(statuses, only) -> Vec<u64>`: the bulk form for the
  toolbar, returning the ids it archived so Undo can unarchive them.
- [x] Remove keeps the downloaded file. `remove` already leaves a completed
  file in place; check `discard_files` for every finished status and
  document the rule in the doc comment.
- [x] `add_tags(ids, tags)` and `remove_tags(ids, tags)`; `add_with` options
  gain `tags`. Invalid tags (empty, too long, too many) return
  `Error::InvalidRequest`.
- [x] Doc comments (`///`) on every new public item.
- [x] Tauri commands for each; mirror the fields in `gui/src/lib/types.ts`
  and the commands in `gui/src/lib/api.ts`.
- [x] Decide what happens to `clear`: **remove it** with its Tauri command,
  `api.ts` wrapper and tests (owner, 2026-11-10). Nothing calls it once the
  GUI archives instead, and a permanent bulk delete invites a frontend to
  skip the archive step. The removal lands in PR B with the GUI switch, so
  PR A ships a working app on its own; its file-rule cases move to `remove`
  tests.

### PR B — GUI
- [x] Sidebar: Archived entry with count; Tags section with counts.
- [x] Filter logic: archived rows out of All, status and folder views and
  counts; the Archived view shows only archived rows; tag filter.
- [x] Toolbar: Archive finished with the undo toast; in the Archived view,
  Unarchive and Remove for the selection.
- [x] Context menu: Archive / Unarchive, Remove (Archived view only),
  Tags….
- [x] Add dialog tag input; tag editor in the details pane Overview.
- [x] Ctrl+A select all, if not already present.
- [x] Update the `clear` e2e spec and any copy that says "Clear finished".
- [x] Delete `Manager::clear`, the `clear` Tauri command, `api.clear` and
  `tests/clear.rs`, keeping its file-rule cases as `remove` tests.
- [x] Remove confirmation dialog with "Don't ask again" and its Settings
  toggle.
- [x] Add dialog: "In your archive" duplicate note.
- [x] Tags section follows the Archived view.

## Acceptance criteria
_Standing gates (fmt/clippy/test/CI) live in AGENTS.md's Definition of Done
and apply here without being restated. E2E specs follow AGENTS.md's WebKitGTK
rules (`waitForGone`, `waitForText` for truncated text, no repeated
characters through `browser.keys`, Shift via a dispatched `KeyboardEvent`).
Ordering assertions use Manager status, not progress bytes._

**Engine (`cargo test`):**
- [x] `archive` sets `archived` on completed, failed and cancelled
  downloads; a queued, downloading or paused one is left unchanged and not
  counted. `unarchive` reverses it. (`tests/archive.rs`)
- [x] `archive_finished([completed, cancelled], None)` archives exactly those
  and returns their ids. (`tests/archive.rs`)
- [x] `remove` on an archived completed download drops the record and the
  file is still on disk, byte-identical to the fixture. (`tests/archive.rs`)
- [x] `add_tags` / `remove_tags`: case-insensitive duplicates are ignored,
  order is kept, and empty, 33-character and 17th tags are rejected with
  `InvalidRequest`. `add_with` stores the given tags. (`tests/tags.rs`)
- [x] Persistence round-trip: a store with archived and tagged records saves
  and reloads equal, through `Manager::load`. (`store.rs` unit test and
  `tests/archive.rs`)
- [x] Old state upgrade: a v1 `state.json` fixture (written by the current
  release) loads with `archived = false` and no tags, and saves as
  `version: 2`. (`store.rs` unit test)
- [x] A `version: 3` document fails to load with an error that names the
  version. (`store.rs` unit test)
- [x] `remove` on an archived failed download deletes its `.part` and
  sidecar. Resuming an archived download unarchives it. (`tests/archive.rs`)

**Frontend (Vitest):**
- [x] Filters: archived rows are excluded from All, every status filter and
  every folder filter; the Archived view holds exactly the archived rows.
- [x] Counts: status, folder, Archived and tag counts match the rows each
  filter shows, for a fixture snapshot mixing archived, tagged and plain
  downloads.
- [x] Tag list: derived from non-archived downloads only, deduplicated
  ignoring case; in the Archived view, from archived downloads only, and an
  archived tag filter shows exactly the archived rows with that tag.
- [x] Duplicates: a pasted URL matching an archived download gets the "In
  your archive" note and is still added.
- [x] Remove confirmation preference: defaults to asking, "Don't ask again"
  turns it off, and the Settings toggle turns it back on.

**E2E (`gui/e2e/specs/`, against the fixture server):**
- [x] **Archive finished + undo:** with a completed and a stopped download,
  Archive finished removes both rows from All and the Archived count reads
  2. Undo brings them back and the count returns to 0. (`archive.e2e.ts`)
- [x] **Archived view + unarchive:** an archived row appears only under
  Archived; Unarchive returns it to All and to Done. (`archive.e2e.ts`)
- [x] **Remove keeps the file:** removing an archived completed download
  from the Archived view drops the row, and the file is still in the
  download directory (checked from the spec with Node `fs`).
  (`archive.e2e.ts`)
- [x] **Tag at add:** adding a URL with tag `work` shows the tag on the row
  and a `work` entry under Tags with count 1. (`tags.e2e.ts`)
- [x] **Tag filter counts:** with two downloads tagged `work` and one
  untagged, the `work` filter shows exactly two rows and its count reads 2;
  archiving one drops the count to 1. (`tags.e2e.ts`)
- [x] **Archived tags:** in the Archived view the Tags section lists only
  archived tags, and selecting one narrows the Archived view.
  (`tags.e2e.ts`)
- [x] **Remove asks first:** Remove opens the confirmation; Cancel keeps the
  row, Remove drops it; with "Don't ask again" ticked the next Remove skips
  the dialog, and the Settings toggle brings it back. (`archive.e2e.ts`)
- [x] The updated `clear` spec (now Archive finished) and the other existing
  specs pass.

## Open questions
All resolved by the owner on 2026-11-10:
1. **Failed downloads' leftover partials on Remove:** delete them (today's
   `remove` rule): a failed download's `.part` isn't a usable file. The
   downloaded file and a partial kept by Stop always stay. Every Remove asks
   for confirmation first, with "Don't ask again" and a Settings toggle to
   turn it back on.
2. **Archived downloads and duplicate detection:** flag them with a
   distinct note ("In your archive") and still add the URL, so it can be
   downloaded again. Records whose file was later deleted or moved (a
   missing-file marker, relinking, moving through rdm) go to the backlog.
3. **Tags in the Archived view:** tags follow the view. In Archived the Tags
   section lists archived tags with archived counts and narrows the Archived
   view; elsewhere it lists non-archived tags only.
4. **`clear`:** removed (see PR A's tasks).
