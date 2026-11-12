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

Pick from the top. Items 1–2 are single PRs; item 3 becomes the next
milestone ticket when it's picked up.

1. **Mark-of-the-Web on Windows** (from post-download safety checks): write
   a `Zone.Identifier` stream (ZoneId=3, HostUrl) on each finished file, as
   browsers do, so SmartScreen, Office protected view and antivirus treat it
   as downloaded. Today rdm's files skip those checks.
2. **Re-probe after a settle** (M4 follow-up): once settled, or frozen by a
   speed limit that is later lifted, ramping never tries again for that
   attempt, so lifting a limit doesn't bring connections back.
3. **Smarter connections** (next milestone): the rest of the M4 ramping
   follow-ups in one milestone, tuned with live smoke runs.
   - *Mid-download connection shedding*: ramping only grows then settles
     once; drop a connection when aggregate throughput falls later on.
   - *Slow connection setup*: a new connection is only treated as "warming
     up" for the window it joins in; on high-latency hosts its setup can
     spill into the next, judged window and stop ramping early.
   - *Ramp window tuning*: with the 1 s window a download needs ~7 s to
     reach 4 connections; a live smoke run decides whether it should shrink.
   - *Rate-limited hosts*: a host that limits request rate (e.g. OVH) costs
     a 429 and its Retry-After wait per segment request; after a rate limit,
     fetch the remainder in one open-ended request.
4. **Missing and moved files**: spot a finished download whose file was
   deleted or moved, relink it ("Update file reference"), or move files
   through rdm so the record follows them; keep a record whose file is gone
   as a note of what was downloaded.
5. **Code signing** (cut from M6): sign the Windows binaries and installers
   (e.g. Azure Trusted Signing) and sign and notarise the macOS app (Apple
   Developer Program), so SmartScreen and Gatekeeper stop warning. Has a
   running cost; move it up when rdm is promoted beyond its current users.
6. **Proxy support** (step 1 of per-app VPN routing, below): `--proxy` plus
   a GUI setting for SOCKS5/HTTP proxies.

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
  downloads after N days; auto-tag rules by host or file type; tag rename,
  merge and colours.
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
  without changing the whole machine. (1) First, SOCKS5/HTTP proxy support
  (`--proxy` plus a GUI setting; reqwest supports SOCKS): many VPN services
  expose a SOCKS5 endpoint, and it also helps behind corporate proxies.
  (2) Optionally, bind to a chosen network interface (e.g. the VPN tunnel
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
ramping follow-ups moved to "Up next". These are low priority.

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
