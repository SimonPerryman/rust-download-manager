# rdm — Roadmap

A download manager in Rust. This file is the **index** and
scope-control tool: it maps features to milestones so we can defer things at a
high level ("that's M4") instead of re-arguing each one. Each milestone has its
own ticket in `milestones/` with tasks and runnable acceptance criteria.

## Design principles (hold across all milestones)

- **Engine / frontend split.** The core download engine is a library with a
  clean API. The CLI (and later the GUI) are thin frontends that call it. Never
  put logic in the frontend that belongs in the engine.
- **The engine's core loop assumes control.** Even before we implement them,
  the "grab chunk → write chunk" loop is written so a download's *rate can be
  throttled* and it *can be paused/cancelled* mid-stream. Leaving these hooks
  in from day one keeps speed-limiting and queueing cheap to add later.
- **State is persistable.** A download knows enough about itself (URL, bytes
  written, ETag/Last-Modified, target path) to be written to disk and resumed
  after a full restart.
- **Everything is verifiable.** Every feature ships with a runnable test;
  acceptance criteria are binary pass/fail, not "looked fine". Default tests
  are deterministic and offline (local server); one live smoke test exercises
  the real network on demand.

## Milestones

| ID | Milestone | Status | Ticket |
|----|-----------|--------|--------|
| M0 | Walking skeleton | done | [[milestone-00-walking-skeleton]] |
| M1 | Robust single-connection (MVP core) | done | [[milestone-01-robust-single-connection]] |
| M2 | GUI (first post-MVP) | done | [[milestone-02-gui]] |
| M3 | Multi-segment parallel downloads | done | [[milestone-03-multi-segment]] |
| M4 | Queue & throughput management | done | [[milestone-04-queue-throughput]] |
| M5 | GUI rethink — Console layout | done | [[milestone-05-gui-rethink]] |
| M6 | Releases & install | done | [[milestone-06-releases-install]] |
| M7 | Archive & tags | done | [[milestone-07-archive-tags]] |
| M8 | Diagnostics — finding the bug when rdm crashes | done | [[milestone-08-diagnostics]] |
| M9 | In-app updater | done | [[milestone-09-in-app-updater]] |
| M10 | Smarter connections | done | [[milestone-10-smarter-connections]] |
| M11 | Missing and moved files | done | [[milestone-11-missing-and-moved-files]] |
| M12 | Batch downloads | todo | [[milestone-12-batch-downloads]] |

**MVP = M0 + M1** — lean, single-connection, CLI-only. Kill-and-resume works.

Dependency notes: M2, M3 and M4 all depend only on M1, not on each other, so
their order is flexible. M2 (GUI) is sequenced first post-MVP by choice; M3 is
the parallel-download speed feature; M4 is the day-to-day queue/throttle layer.
M6 (prebuilt releases) follows M5 so the first release ships the new GUI. M7
needs M5's details pane, context menu and multi-select, and comes after M6.
M8 (diagnostics) follows the first releases, now that bugs show up on
machines the developer can't reach. M9 (in-app updater) builds on M6's
release pipeline and doesn't depend on M8, so the two can land in either
order.

## Up next (in priority order)

Pick from the top; each is described in the backlog below.

1. **Batch** ([[milestone-12-batch-downloads]]): URL lists from the CLI and
   GUI, numbered patterns such as `file[01-20].zip`, and batches you can
   see and retry. Site mirroring isn't planned. Still to ship: the batch
   speed limit, then the docs pass.
2. **Scheduling & automation**: start/stop at a time, bandwidth schedules,
   post-download actions.
3. **Protocols beyond HTTP**: FTP/SFTP, BitTorrent, metalink/mirror failover.
4. **Content-aware / media**: HLS and DASH manifests, muxed to one file.
5. **Browser integration**: native-messaging host and extension.

Deferred: **Code signing** (cut from M6). Sign the Windows binaries and
installers and sign and notarise the macOS app, so SmartScreen and
Gatekeeper stop warning. The requirements are high, so it waits. Apple's
program is a paid yearly membership, and Microsoft's Artifact Signing (the
Azure service) is open to individuals only in the US and Canada (UK
organisations qualify), so the alternative is a paid OV certificate from a
certificate authority. Move it up when rdm is promoted beyond its current
users.

Shipped from this list: Mark-of-the-Web on Windows, and the smarter-connections
work ([[milestone-10-smarter-connections]]: re-probe after a settle, shedding
on a sustained drop, slow-setup warm-up, one wait for rate-limited hosts), and
missing and moved files ([[milestone-11-missing-and-moved-files]]).

## Backlog (not yet scheduled — parked on purpose)

The first five items of "Up next" come from here; beyond those, roughly in
priority order, with the large or unrequested items last.

- **Batch**: site mirroring (the rest is [[milestone-12-batch-downloads]])
- **Checksum lists**: import a `SHA256SUMS` file, verify many downloads at
  once, export sums for finished files.
- **Post-download safety checks, beyond Mark-of-the-Web** (optional): a
  checksum only proves we got the bytes the server sent, not that they're
  safe or what they claim to be. Candidates: attach through
  `IAttachmentExecute` so the registered antivirus scans each file;
  magic-byte sniffing against the extension (flag a "video" that's an
  executable, or `.mp4.exe`); warn when an archive holds executables or
  scripts; a user-configured post-download command (e.g. a scanner).
- **Archive and tag automation (cut from M7)**: auto-archive finished
  downloads after N days; auto-tag rules by host or file type; tag suggestions
  that propose tags for everything already in the list, with a dry-run
  view of which tag each file would get and which files fall under each
  tag, before anything is applied; tag rename, merge and colours.
- **Updater extras (cut from M9)**: "Skip this version" in the update
  notice; in-app updates for `.deb` installs (the plugin can, but it needs
  an admin password prompt that not every desktop has).
- **Download list layout ideas** (from owner review of missing files):
  finished rows leave Speed, Time left and Connections empty, so weigh
  fixed columns (today), per-view columns (e.g. Completed shows "Completed
  at" and location) or user-chosen and reorderable columns, bearing in mind
  that columns changing between filters looks jumpy. Sorting by column
  header already exists.
- **GUI desktop integration (cut from M5)**: a free-space readout in the
  status bar (needs a `free_space(dir)` command), native OS notifications
  (Tauri notification plugin), tray icon, start with the system.
- **GUI polish (post-M2)**: density variants (compact/full), alternate layouts/
  themes beyond the shipped slick light+dark, richer per-download detail views,
  drag-and-drop / clipboard-URL add. M2 ships one clean slick design; these are
  deliberately deferred.
- **Per-app VPN routing** (optional): route only rdm's traffic through a VPN
  without changing the whole machine. (1) Done: SOCKS5/HTTP proxy support
  (`--proxy` and the GUI's Proxy setting), since many VPN services expose a
  SOCKS5 endpoint. (2) Optionally, bind to a chosen network interface (e.g. the VPN tunnel
  adapter) with a kill switch: pause downloads rather than fall back to the
  normal connection if that interface goes away. Easier on Linux/macOS;
  Windows needs more work. Out of scope: an embedded VPN client, which is
  security-sensitive, needs credentials and duplicates the VPN app's own
  split tunnelling.
- **Scheduling & automation**: start/stop at a time, bandwidth schedules,
  post-download actions (run command, shut down, auto-organize by type)
- **Persistence at scale**: migrate the download-list + history store from JSON
  v1 (chosen at M2) to SQLite (`sqlx`/`rusqlite`) once query/filter/search or
  large histories justify it — likely alongside M4's queue work. JSON v1 sits
  behind a thin store boundary at M2 so this is a contained swap, not a rewrite.
- **Crash reporting/telemetry (opt-in) (cut from M8)**: send crash reports
  automatically (e.g. Sentry), plus native minidumps for crashes that aren't
  Rust panics. Needs a server, a privacy policy and explicit opt-in; until
  then users paste Copy diagnostics into a bug report.
- **Browser integration**: native-messaging host + extension, "download with",
  clipboard URL monitoring, protocol handler
- **Protocols beyond HTTP**: FTP/SFTP, BitTorrent, metalink/mirror failover
- **Content-aware / media**: detect HLS (`.m3u8`) / DASH (`.mpd`) manifests,
  download segments, mux with ffmpeg → single playable file (effectively a
  second product; large surface area)

## Follow-ups (small, from shipped milestones)

Known gaps left deliberately out of a milestone's scope. Each is small enough
to be a single PR; promote one into a ticket when it's picked up. The M4
ramping follow-ups shipped in M10; ramp window tuning was dropped (below). These are low priority.

- **Ramp window tuning** (M10 follow-up): dropped for now. With the 1 s
  window a download needs ~7 s to reach 4 connections, but a live run on
  2026-11-14 (1000/500/250 ms builds) showed no difference on a link one
  connection already fills, and settling it needs a server that caps each
  connection. The window is the `RAMP_WINDOW` constant in
  `src/engine/ramp.rs`.
- **Add-time checksum "checking" state** (M5 PR A): the engine hashes inside
  finalize, so `checksum_state` never shows `pending` for it; an engine hook
  could publish it. Fold it into the next checksum work.
- **Automated pin bumps for the rest** (CI pins): Dependabot already moves
  action SHAs; runner images, the Rust toolchain and `node-version` are
  bumped by hand. Renovate could propose those too.
- **Move CI to Ubuntu 26.04** (CI pins): the e2e job's `webkit2gtk-driver`
  is `webkitgtk-webdriver` there; check the other Tauri deps and bump the pin
  in its own PR. No rush: 24.04 is supported until 2029.
- **Missing-file polish** (missing and moved files): opening a missing row
  toasts the engine's wording, "download N's file is no longer on disk";
  reword it to point at Locate…. The Locate… row button lines up with
  the labelled Open button but not with the small icon Open shown after a
  row has been opened; left as is, since a missing row never shows the
  icon Open. Locate… accepts any existing file; only
  Find it checks name and size.

### From batch downloads, in priority order

1. **Flaky ramp tests** (#156): tests that wait on real time for a ramp
   window time out on CI. Three different ones failed on one PR's Windows
   job (`tests/common/ramp.rs:126`) and one on Ubuntu earlier, while local
   runs pass. They block merges, so fix the waits before more PRs pile up.
2. **Windows e2e failure** (#162): `missing.e2e.ts` "Find file lists
   candidates when the file is in several folders" fails with "click
   intercepted" on Windows, on main too. Linux CI doesn't show it.
3. **Add to existing batch**: typing the name of a batch that already
   exists offers to add the new links to it and its folder. Today the add
   makes a second batch with the folder `name (1)`.
4. **Already-downloaded detection**: flag a link that looks already
   downloaded, and give the CLI a no-redownload option that prints the
   existing file's location, with strict (size and checksum) and loose
   (name and size) matching. It also fixes a rerun of `rdm -i` downloading
   finished files again as `name (1)`. Larger than the rest.
5. **Error banner inset**: a failed row's banner lines up with the file
   name, as it always has. Revisit if it still looks off, across every
   view.
6. **Batch name length**: capped at 64 characters; a one-line change if it
   should be shorter.

## Candidate crates

`reqwest` (HTTP), `tokio` (async), `futures` (concurrency), `clap` (CLI),
`indicatif` (CLI progress), `serde` (config/state), `sqlx`/`rusqlite`
(persistence), `sha2` (checksums). GUI: Tauri (chosen at M2).
- **Show the queue order** (GUI): make a queued download's place in the
  queue visible at a glance (for example a position number or a quiet
  ordinal), designed with the Refactoring UI principles.
