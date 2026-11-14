---
id: M11
title: Missing and moved files
status: todo
depends-on: [M5, M7]
tags: [engine, gui, persistence, testing]
---

## Goal
rdm notices when a finished download's file has been deleted or moved,
keeps its record, and lets the user relink it or remove it, with help
finding where the file went.

## Context / rationale
Today a finished download whose file is gone still looks fine until Open
fails. A download manager's list is a record of what you fetched, so the
record stays; only the link to the file needs repair. Roadmap index:
[[roadmap]]. A visual mock-up of the states is kept locally (not in the
repo) in `.ai-cache/mockup-missing-files.html`.

## Decisions (workers follow these; escalate only if one can't hold)
- **Missing is derived, not a new engine state.** The download stays
  `finished`; the Manager reports a `file_missing` overlay. A missing row
  counts under both Completed and the new Missing filter.
- **When to check:** on app start, when the window regains focus (at most
  once every ~30 s), and right before Open / Show in folder. Always on a
  background task, never the UI thread; finished downloads only; list each
  folder once instead of one lookup per file; network and removable drives
  only when the user looks at or acts on a record from them.
- **The flag:** the row dims and shows a warning icon with "File missing"
  in place of size and speed. Colour is never the only signal.
- **Details pane:** a banner with the old path, **Locate…** as the primary
  action (an OS file picker), **Remove from list** as a quieter secondary
  (the record only; the file is already gone).
- **After one relink,** offer to check the new folder for the other
  missing downloads that were in the same old folder.
- **Find it** (bounded search): the record's old folder, the folders of the
  other downloads, then the default download folder. A match is the same
  file name and exactly the same size; if the name changed, a unique size
  plus extension is a "possible match". When several candidates share a
  size and the record has a stored checksum, hash only those to decide. No
  whole-drive search.
- **Never relink silently.** A found file is offered as a "we think it's
  this" suggestion the user clicks to accept.
- **Where Find it lives:** the row's right-click menu and the details pane.
  A "Find all" button appears only in the Missing filter view.
- All logic lives in the engine; the GUI only displays and calls it.

## Scope
**In:** detection; the flag; Locate… and Remove from list; the same-folder
offer; Find it with candidates, progress and cancel; the Missing filter;
docs and screenshots.

**Out (deferred):**
- An automatic-relink setting (only when exactly one candidate matches).
- Moving files through rdm so the record follows them.
- Hashing every finished download so renames can be matched by checksum.
- A CLI relink command (revisit if the CLI shares the GUI's download list).

## Tasks
- [ ] `missing-detect`: engine check, Manager overlay and event, tests with
      temporary folders
- [ ] `missing-flag-relink`: dimmed row, details banner, Missing filter,
      Locate…, Remove from list
- [ ] `missing-same-folder`: offer to check the new folder; multi-select
      Locate folder…
- [ ] `missing-find-it`: bounded search, candidates panel, progress,
      cancel, context menu and "Find all"
- [ ] `missing-docs`: `docs/gui.md`, `docs/how-it-works.md`, screenshots

## Acceptance criteria
- [ ] `cargo test` passes, with fixture tests for detection, relink, the
      same-folder offer and the search (name+size, unique-size rename,
      checksum tie-break, bounded folders, cancel).
- [ ] The GUI unit tests and a webview e2e spec cover the missing row,
      Locate…, Remove from list and the suggestion button.
- [ ] No check runs on the UI thread, and a record on an unreachable drive
      does not block the list.
- [ ] `docs/gui.md` and `docs/how-it-works.md` describe the behaviour, and
      the screenshots are regenerated.
