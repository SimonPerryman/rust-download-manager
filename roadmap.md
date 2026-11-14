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
| M11 | Missing and moved files | todo | [[milestone-11-missing-and-moved-files]] |

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

Pick from the top. Items 2–3 need the owner before any code: live
measurements and a paid account respectively.

1. **Missing and moved files** ([[milestone-11-missing-and-moved-files]]):
   spot a finished download whose file was deleted or moved, keep its
   record, and let the user relink it (Locate…, or Find it) or remove it.
2. **Ramp window tuning** (M10 follow-up): with the 1 s window a download
   needs ~7 s to reach 4 connections. Parked, low value: shrinking it saves
   a fixed ~2-3 s per download, which only shows on 100 MB to 1 GB files
   from hosts that cap each connection. A live run on 2026-11-14 (Ubuntu
   24.04 ISO, 1000/500/250 ms builds) showed nothing: one connection
   already fills a ~460 Mbit/s link, so every variant tried extra lanes and
   dropped them, and 250 ms stopped one lane earlier (the noise risk). To
   settle it, measure against a server that caps each connection: a local
   fixture with a per-connection rate cap, or a distant mirror. Compare
   time to 4 lanes, final speed and total time, over several runs. The
   window is the `RAMP_WINDOW` constant in `src/engine/ramp.rs`, so each
   variant is a rebuild.
3. **Code signing** (cut from M6): sign the Windows binaries and installers
   (e.g. Azure Trusted Signing) and sign and notarise the macOS app (Apple
   Developer Program), so SmartScreen and Gatekeeper stop warning. Has a
   running cost; move it up when rdm is promoted beyond its current users.

Shipped from this list: Mark-of-the-Web on Windows, and the smarter-connections
work ([[milestone-10-smarter-connections]]: re-probe after a settle, shedding
on a sustained drop, slow-setup warm-up, one wait for rate-limited hosts).

## Backlog (not yet scheduled — parked on purpose)

Roughly in priority order: batch and checksum work first, the large or
unrequested items last.

- **Batch**: wildcard / URL-list downloads, site mirroring
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
ramping follow-ups shipped in M10; window tuning is in "Up next". These are low priority.

- **Add-time checksum "checking" state** (M5 PR A): the engine hashes inside
  finalize, so `checksum_state` never shows `pending` for it; an engine hook
  could publish it. Fold it into the next checksum work.
- **Automated pin bumps for the rest** (CI pins): Dependabot already moves
  action SHAs; runner images, the Rust toolchain and `node-version` are
  bumped by hand. Renovate could propose those too.
- **Move CI to Ubuntu 26.04** (CI pins): the e2e job's `webkit2gtk-driver`
  is `webkitgtk-webdriver` there; check the other Tauri deps and bump the pin
  in its own PR. No rush: 24.04 is supported until 2029.

## Candidate crates

`reqwest` (HTTP), `tokio` (async), `futures` (concurrency), `clap` (CLI),
`indicatif` (CLI progress), `serde` (config/state), `sqlx`/`rusqlite`
(persistence), `sha2` (checksums). GUI: Tauri (chosen at M2).
- **Say "Queued", not "Waiting"** (GUI copy): the row, filter, details pane
  and view title call a queued download "Waiting". "Queued" is the term for
  it. The "Next up" pill stays. Touches `rowView`, `laneWork`, `details`,
  `copy` and `filters` plus their tests, the e2e specs and the screenshots.
- **Show the queue order** (GUI): make a queued download's place in the
  queue visible at a glance (for example a position number or a quiet
  ordinal), designed with the Refactoring UI principles.
