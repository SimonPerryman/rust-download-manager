---
id: M12
title: Batch downloads
status: todo
depends-on: [M4, M5, M7]
tags: [engine, cli, gui, testing]
---

## Goal
Add many downloads in one step, from a list of links or a pattern such as
`img[001-040].jpg`, from either the CLI or the GUI, with one shared set of
rules for what a list means.

## Context / rationale
The GUI's Add dialog already takes one link per line (M5): it previews each
file, skips lines that aren't links and repeats, flags links already in the
list or archive, and carries on past a link the engine rejects. Three gaps
remain:

- The list rules live in the frontend (`gui/src/lib/batch.ts`), so the CLI
  can't use them. That also breaks the engine/frontend split.
- The CLI takes exactly one URL per run.
- A batch has no identity once added: its downloads are just rows, so a
  user can't see how a batch is going, retry only its failures, or filter to
  it.

Numbered file sets (`part[01-20].zip`) are the common case that pasting
doesn't serve well. Roadmap index: [[roadmap]].

## Decisions (workers follow these; escalate only if one can't hold)
- **List rules live in the engine** (`engine::batch`), as pure functions the
  CLI and GUI both call. The GUI's preview asks the engine through a Tauri
  command instead of parsing in TypeScript; `batch.ts` keeps only
  presentation.
- **List format:** links split on newlines and whitespace, and on a comma or
  pipe that is followed by another `http(s)://` link (so a comma inside one
  URL stays put); blank lines and `#` comments ignored; `http` and `https`
  only. No per-line options.
- **Duplicates:** a link repeated within one list is added once, and the
  preview shows it once with its count (×2). A link already in the list or
  archive is flagged in the GUI preview but still added (as today); the CLI
  adds it without comment. Skipped lines are counted in the summary and
  listed only on request.
- **Per-item failure never stops a batch.** A line that isn't a link or a
  link the engine rejects is reported and skipped; the rest go in. The CLI
  exits non-zero if any item failed.
- **Patterns:** a `[a-b]` range in the path or query expands to every value
  between, curl-style. Numbers keep their zero padding (`[01-20]` gives
  `01`…`20`); single letters work too (`[a-f]`). One pattern per link, in
  the path or query only, never the host. The count is worked out from the
  range, never by generating it, and a list over 1000 links is refused with
  its count. Rdm can't know which numbers exist: every link is added and a
  missing one fails on its own row.
- **Batches are queued like any download.** Each item is a normal download:
  it waits behind the queue cap, takes the global speed limit, and a
  per-download limit applies to each item. An optional batch limit caps
  the batch's items combined (slice 5). No new scheduler.
- **Folder:** with two or more links, "New folder for this batch" puts
  them in a subfolder named by the batch name (dated if unnamed). It is
  ticked once a name is typed and unticked otherwise, so naming a batch is
  the signal. Name clashes use the existing `name (1)` rule.
- **Batch identity (slice 4):** each add of two or more links gets a batch
  id and an optional label saved on its downloads (store schema v3,
  optional fields, older stores load unchanged). The list shows a batch
  as a group with a rollup ("12 of 40 done"), and **Retry failed** acts on
  that batch only. A Grouped / Flat switch in the list toolbar turns the
  grouping off (remembered); Flat is today's list. The sidebar gains a
  Batches section that filters to one batch. Batches and tags stay separate:
  a tag is a free label a user edits, a batch is fixed at add time.
- **CLI form:** `rdm -i urls.txt` (`-i -` reads stdin). The `<URL>`
  argument stays for one link and may be repeated; `-i` and links can be
  mixed. A batch runs through the Manager so the queue cap applies, and the
  CLI shows one line per item plus a summary. `-o` is refused with more than
  one link, and so is `--checksum`.
- **Mirroring is out.** Crawling a site is a different product (robots,
  depth, rewriting links), so it isn't scheduled here.

## Scope
**In:** engine list parsing and pattern expansion; GUI preview via the
engine; CLI `-i` and repeated links; batch id, label, group row and Retry
failed; opt-in folder per batch; docs and screenshots.

**Out (deferred):**
- Site mirroring or recursive fetch (not planned).
- Per-line options in a list file (output name, checksum).
- Importing a checksum list alongside a batch (→ "Checksum lists" in the
  roadmap backlog).
- Brace sets (`{a,b,c}`) and more than one pattern per link.
- Scheduling a batch for later (→ "Scheduling & automation" in the roadmap
  backlog).

## Tasks
Each is its own PR, in this order. Slices 4 and 5 need the owner's
go-ahead after slice 3.
- [x] `batch-engine`: `engine::batch` (parse, dedupe, report bad lines),
      Tauri preview command, GUI preview switched over, `batch.ts` trimmed,
      tests
- [x] `batch-cli`: `-i` / `-i -` and repeated `<URL>`, Manager-backed run,
      per-item lines, summary and exit code, `docs/cli.md`
- [x] `batch-patterns`: range expansion in the engine, preview count in the
      Add dialog, 1000-link cap, docs
- [x] `batch-group`: batch id and label (schema v3), group row with rollup,
      Retry failed, opt-in folder per batch, e2e spec and screenshots
- [x] `batch-size-probe`: a size probe (`HEAD`, else a one-byte range) when a
      download is added, so a waiting row shows its size
- [ ] `batch-limit`: optional batch-wide speed cap shared by a batch's
      items (a per-batch limiter in the engine), field in the Add dialog's
      More options, test that the items' combined rate stays under it
- [ ] `batch-docs`: `docs/gui.md`, `docs/cli.md`, `docs/how-it-works.md`,
      README highlight, screenshots

## Acceptance criteria
- [ ] `cargo test` passes, with offline fixture tests (fixture server, no
      real network) for: comment and blank-line handling; repeats within a
      list; a bad line and a rejected link not stopping the rest; zero-padded
      numeric and letter ranges; the 1000-link cap; a batch of more links
      than the queue cap running at most the cap at once (asserted on
      Manager status, not progress bytes); name clashes in one folder; a
      v2 store loading under v3.
- [ ] `rdm -i urls.txt` against the fixture server downloads every listed
      file, prints one line per item and a summary, and exits non-zero when
      one item is a 404 while the others still finish (`tests/cli.rs`).
- [ ] `rdm -i -` reads the same list from stdin; `-i` with `-o` or
      `--checksum` is a usage error.
- [ ] The GUI unit tests cover the preview rendering from engine output,
      and a webview e2e spec covers pasting a list, a pattern, the group
      row's rollup and Retry failed.
- [ ] No list parsing remains in `gui/src`.
- [ ] `docs/cli.md`, `docs/gui.md` and `docs/how-it-works.md` describe the
      behaviour, the README highlights mention it, and the screenshots are
      regenerated.
