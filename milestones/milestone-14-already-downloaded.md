---
id: M14
title: Already-downloaded detection
status: todo
depends-on: [M11, M12]
tags: [engine, cli, gui, testing]
---

## Goal
Notice when a link has already been downloaded, so rdm flags it in the GUI
and the CLI can skip it and print where the existing file is.

## Context / rationale
Rerunning `rdm -i links.txt` downloads every finished file again as
`name (1).ext`, and the GUI only flags links already in the list or archive,
not files that are on disk. Roadmap index: [[roadmap]]. Matching reuses the
name and size check from Find file (M11) and the batch list rules (M12).

## Decisions (workers follow these; escalate only if one can't hold)
- **Matching lives in the engine** (`engine::existing`), a pure function the
  CLI and GUI both call. Neither frontend matches anything itself.
- **Where it looks.** The CLI has no download list, so it checks the disk
  only: the file the download would be saved to in the destination folder.
  The GUI checks both, in parallel: the download list and archive (survives
  file moves), and the target path on disk (survives list clears). No folder
  scans; a match is a lookup or a single stat.
- **Loose (default):** same file name and same size as the server reports
  (a HEAD, or the size the preview already fetched). Cheap.
- **Strict:** loose, plus a SHA-256 match. Uses `--checksum` when given,
  otherwise hashes the existing file and compares with the recorded digest of
  the matching list entry. A strict check with nothing to compare against is
  reported as a loose match, saying so.
- **CLI:** `--skip-existing[=loose|strict]`, opt-in so existing scripts don't
  change. On a match: no download, print `Already downloaded: <path>`, count it
  as `skipped` in the summary, exit code `0` (a rerun is idempotent: a link
  that was already there is a success). `--json` emits a `skipped` event with
  `reason: "already_downloaded"`, `path` and `match: "loose"|"strict"`.
- **`rdm -i` rerun:** with `--skip-existing`, finished files are skipped and
  the rest download, so rerunning a list resumes it. Without the flag nothing
  changes; docs/cli.md replaces the "downloaded again" warning with this recipe.
- **GUI:** the Add dialog preview flags a match with a chip ("Already
  downloaded") and un-ticks it by default. The row offers Download anyway
  (re-tick) and Show in folder. Loose matching only; no setting.
- **Add to batch (#210):** flagged links are un-ticked, so they are not added
  to the batch unless the user re-ticks them.

## Scope
**In:**
- Engine matcher (history/list + disk, loose and strict), tests.
- CLI `--skip-existing`, summary, exit code, `--json` event, `--help`,
  docs/cli.md.
- GUI Add dialog chip, un-tick, Show in folder, docs/gui.md.
- docs/how-it-works.md section on how a match is decided.

**Out (deferred):**
- Searching other folders for a moved file (Find file already does this).
- Deleting or replacing the existing file.

## Tasks
- [ ] Engine: `existing` module with matcher and fixture tests
- [ ] CLI: flag, output, exit code, `--json` event, `--help`, docs
- [ ] Tauri command and Add dialog chip, un-tick, Show in folder, docs
- [ ] Update README highlights and roadmap row

## Acceptance criteria
- [ ] `cargo test` passes, including `tests/existing.rs` (fixture server):
  loose match skips, differing size does not, strict needs the digest.
- [ ] A rerun of `rdm -i list.txt --skip-existing` after a full run downloads
  nothing, prints one `Already downloaded:` line per file and exits `0`.
- [ ] The same rerun without the flag behaves as before.
- [ ] `--json` emits the `skipped` event with `reason`, `path` and `match`.
- [ ] GUI e2e: a link whose file is on disk shows the chip, is un-ticked, and
  re-ticking it downloads it.
- [ ] `rdm --help`, docs/cli.md, docs/gui.md and docs/how-it-works.md updated.
