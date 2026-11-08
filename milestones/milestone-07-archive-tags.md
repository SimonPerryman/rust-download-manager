---
id: M7
title: Archive & tags
status: todo
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
  - A new sidebar entry, **Archived**, with a live count, lists them. From
    there they can be unarchived (they return to their status and folder
    views) or removed.
  - The toolbar's **Clear finished** becomes **Archive finished** (Done and
    Stopped rows, as Clear finished does today). The undo toast stays: Undo
    unarchives the same ids. Archiving is reversible, so there's no delayed
    call as with clear.
  - **Remove** (permanent) moves to the Archived view: a toolbar action for
    the selection and a context-menu item. It drops the record and **never
    deletes the downloaded file**. Del in the main views archives instead of
    removing.
- **Tags:**
  - Rules: trimmed, 1 to 32 characters, unique per download ignoring case,
    kept in the order added. At most 16 per download.
  - Add dialog: an optional tag input; the tags apply to every URL added in
    that batch.
  - Editing later: from the row's context menu ("Tags…") and the details
    pane's Overview, for one download or the whole selection.
  - Sidebar **Tags** section: every tag on a non-archived download, with a
    live count; selecting one filters the table, like the folder filters.
  - A tag shown on the row as a small label (presentation only).
- **Select all** (Ctrl+A) in the table, if M5 PR D doesn't already ship it,
  so "filter by tag, select all, archive" works.

**Out (deferred):**
- Auto-archive finished downloads after N days (backlog).
- Auto-tag rules by host or file type (backlog).
- Tag rename or merge across all downloads, and tag colours (backlog).
- Search by tag in the name filter.
- Any CLI change (see Context).

## Tasks
### PR A — engine and Tauri commands
- [ ] `DownloadRecord` gains `archived` and `tags`; the snapshot carries
  both. Store `SCHEMA_VERSION` = 2; reject newer versions on load.
- [ ] `archive(ids) -> usize` and `unarchive(ids) -> usize`: set or clear
  the flag on finished downloads, persist, publish. Return how many changed.
- [ ] `archive_finished(statuses, only) -> Vec<u64>`: the bulk form for the
  toolbar, returning the ids it archived so Undo can unarchive them.
- [ ] Remove keeps the downloaded file. `remove` already leaves a completed
  file in place; check `discard_files` for every finished status and
  document the rule in the doc comment.
- [ ] `add_tags(ids, tags)` and `remove_tags(ids, tags)`; `add_with` options
  gain `tags`. Invalid tags (empty, too long, too many) return
  `Error::InvalidRequest`.
- [ ] Doc comments (`///`) on every new public item.
- [ ] Tauri commands for each; mirror the fields in `gui/src/lib/types.ts`
  and the commands in `gui/src/lib/api.ts`.
- [ ] Decide what happens to `clear`: keep it as engine API (the GUI stops
  calling it) or remove it with its tests. Note the decision here.

### PR B — GUI
- [ ] Sidebar: Archived entry with count; Tags section with counts.
- [ ] Filter logic: archived rows out of All, status and folder views and
  counts; the Archived view shows only archived rows; tag filter.
- [ ] Toolbar: Archive finished with the undo toast; in the Archived view,
  Unarchive and Remove for the selection.
- [ ] Context menu: Archive / Unarchive, Remove (Archived view only),
  Tags….
- [ ] Add dialog tag input; tag editor in the details pane Overview.
- [ ] Ctrl+A select all, if not already present.
- [ ] Update the `clear` e2e spec and any copy that says "Clear finished".

## Acceptance criteria
_Standing gates (fmt/clippy/test/CI) live in AGENTS.md's Definition of Done
and apply here without being restated. E2E specs follow AGENTS.md's WebKitGTK
rules (`waitForGone`, `waitForText` for truncated text, no repeated
characters through `browser.keys`, Shift via a dispatched `KeyboardEvent`).
Ordering assertions use Manager status, not progress bytes._

**Engine (`cargo test`):**
- [ ] `archive` sets `archived` on completed, failed and cancelled
  downloads; a queued, downloading or paused one is left unchanged and not
  counted. `unarchive` reverses it. (`tests/archive.rs`)
- [ ] `archive_finished([completed, cancelled], None)` archives exactly those
  and returns their ids. (`tests/archive.rs`)
- [ ] `remove` on an archived completed download drops the record and the
  file is still on disk, byte-identical to the fixture. (`tests/archive.rs`)
- [ ] `add_tags` / `remove_tags`: case-insensitive duplicates are ignored,
  order is kept, and empty, 33-character and 17th tags are rejected with
  `InvalidRequest`. `add_with` stores the given tags. (`tests/tags.rs`)
- [ ] Persistence round-trip: a store with archived and tagged records saves
  and reloads equal, through `Manager::load`. (`store.rs` unit test and
  `tests/archive.rs`)
- [ ] Old state upgrade: a v1 `state.json` fixture (written by the current
  release) loads with `archived = false` and no tags, and saves as
  `version: 2`. (`store.rs` unit test)
- [ ] A `version: 3` document fails to load with an error that names the
  version. (`store.rs` unit test)

**Frontend (Vitest):**
- [ ] Filters: archived rows are excluded from All, every status filter and
  every folder filter; the Archived view holds exactly the archived rows.
- [ ] Counts: status, folder, Archived and tag counts match the rows each
  filter shows, for a fixture snapshot mixing archived, tagged and plain
  downloads.
- [ ] Tag list: derived from non-archived downloads only, deduplicated
  ignoring case.

**E2E (`gui/e2e/specs/`, against the fixture server):**
- [ ] **Archive finished + undo:** with a completed and a stopped download,
  Archive finished removes both rows from All and the Archived count reads
  2. Undo brings them back and the count returns to 0. (`archive.e2e.ts`)
- [ ] **Archived view + unarchive:** an archived row appears only under
  Archived; Unarchive returns it to All and to Done. (`archive.e2e.ts`)
- [ ] **Remove keeps the file:** removing an archived completed download
  from the Archived view drops the row, and the file is still in the
  download directory (checked from the spec with Node `fs`).
  (`archive.e2e.ts`)
- [ ] **Tag at add:** adding a URL with tag `work` shows the tag on the row
  and a `work` entry under Tags with count 1. (`tags.e2e.ts`)
- [ ] **Tag filter counts:** with two downloads tagged `work` and one
  untagged, the `work` filter shows exactly two rows and its count reads 2;
  archiving one drops the count to 1. (`tags.e2e.ts`)
- [ ] The updated `clear` spec (now Archive finished) and the other existing
  specs pass.

## Open questions
1. **Failed downloads' leftover partials on Remove:** the downloaded file is
   never deleted, but a failed download's `.part` isn't a usable file.
   Delete it on Remove (today's `remove` rule), or keep everything?
2. **Archived downloads and duplicate detection:** should adding a URL that
   matches an archived download warn, as for a visible one?
3. **Tags in the Archived view:** filter archived rows by tag too, or keep
   the Archived view unfiltered by tag?
