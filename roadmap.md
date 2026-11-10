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
| M8 | Diagnostics — finding the bug when rdm crashes | todo | [[milestone-08-diagnostics]] |

**MVP = M0 + M1** — lean, single-connection, CLI-only. Kill-and-resume works.

Dependency notes: M2, M3 and M4 all depend only on M1, not on each other, so
their order is flexible. M2 (GUI) is sequenced first post-MVP by choice; M3 is
the parallel-download speed feature; M4 is the day-to-day queue/throttle layer.
M6 (prebuilt releases) follows M5 so the first release ships the new GUI. M7
needs M5's details pane, context menu and multi-select, and comes after M6.
M8 (diagnostics) follows the first releases, now that bugs show up on
machines the developer can't reach.

## Backlog (not yet scheduled — parked on purpose)

- **Scheduling & automation**: start/stop at a time, bandwidth schedules,
  post-download actions (run command, shut down, auto-organize by type)
- **Browser integration**: native-messaging host + extension, "download with",
  clipboard URL monitoring, protocol handler
- **Content-aware / media**: detect HLS (`.m3u8`) / DASH (`.mpd`) manifests,
  download segments, mux with ffmpeg → single playable file (effectively a
  second product; large surface area)
- **Protocols beyond HTTP**: FTP/SFTP, BitTorrent, metalink/mirror failover
- **Batch**: wildcard / URL-list downloads, site mirroring
- **Persistence at scale**: migrate the download-list + history store from JSON
  v1 (chosen at M2) to SQLite (`sqlx`/`rusqlite`) once query/filter/search or
  large histories justify it — likely alongside M4's queue work. JSON v1 sits
  behind a thin store boundary at M2 so this is a contained swap, not a rewrite.
- **GUI polish (post-M2)**: density variants (compact/full), alternate layouts/
  themes beyond the shipped slick light+dark, richer per-download detail views,
  drag-and-drop / clipboard-URL add. M2 ships one clean slick design; these are
  deliberately deferred.
- **GUI desktop integration (cut from M5)**: a free-space readout in the
  status bar (needs a `free_space(dir)` command), native OS notifications
  (Tauri notification plugin), tray icon, start with the system.
- **Code signing (cut from M6)**: sign the Windows binaries and installers
  (e.g. Azure Trusted Signing) and sign and notarise the macOS app (Apple
  Developer Program), so SmartScreen and Gatekeeper stop warning.
- **In-app updater (cut from M6)**: the Tauri updater plugin with a signed
  `latest.json` on each release, a Rust-side check on start and an "Update
  available" notice with Install & restart. The update keypair already
  exists. The drafted scope and tests are in the git history of
  [[milestone-06-releases-install]].
- **Archive and tag automation (cut from M7)**: auto-archive finished
  downloads after N days; auto-tag rules by host or file type; tag rename,
  merge and colours.
- **Missing and moved files**: spot a finished download whose file was
  deleted or moved, relink it ("Update file reference"), or move files
  through rdm so the record follows them; keep a record whose file is gone
  as a note of what was downloaded.
- **Post-download safety checks** (optional): a checksum only proves we got
  the bytes the server sent, not that they're safe or what they claim to be.
  Candidates: Windows Mark-of-the-Web (`Zone.Identifier` with ZoneId=3 and
  HostUrl/ReferrerUrl, ideally via `IAttachmentExecute` so SmartScreen,
  Office protected view and the registered antivirus scan apply, as browsers
  do); magic-byte sniffing against the extension (flag a "video" that's an
  executable, or `.mp4.exe`); warn when an archive holds executables or
  scripts; a user-configured post-download command (e.g. a scanner).
- **Crash reporting/telemetry (opt-in) (cut from M8)**: send crash reports
  automatically (e.g. Sentry), plus native minidumps for crashes that aren't
  Rust panics. Needs a server, a privacy policy and explicit opt-in; until
  then users paste Copy diagnostics into a bug report.
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

## Follow-ups (small, from shipped milestones)

Known gaps left deliberately out of a milestone's scope. Each is small enough
to be a single PR; promote one into a ticket when it's picked up.

- **Mid-download connection shedding** (M4): ramping only grows then settles
  once; drop a connection when aggregate throughput falls later on.
- **Re-probe after a settle** (M4): once settled (or frozen by a speed limit
  that is later lifted), ramping never tries again for that attempt.
- **Slow connection setup** (M4): a new connection is only treated as
  "warming up" for the window it joins in; on high-latency hosts its setup
  can spill into the next, judged window and stop ramping early.
- **Ramp window tuning** (M4): with the 1 s window a download needs ~7 s to
  reach 4 connections; a live smoke run decides whether it should shrink.
- **Move CI to Ubuntu 26.04** (CI pins): the e2e job's `webkit2gtk-driver`
  is `webkitgtk-webdriver` there; check the other Tauri deps and bump the pin
  in its own PR.
- **Automated pin bumps** (CI pins): have Dependabot or Renovate propose
  action SHA and image updates, so pinned versions keep getting security fixes.
- **Add-time checksum "checking" state** (M5 PR A): the engine hashes inside
  finalize, so `checksum_state` never shows `pending` for it; an engine hook
  could publish it.

## Candidate crates

`reqwest` (HTTP), `tokio` (async), `futures` (concurrency), `clap` (CLI),
`indicatif` (CLI progress), `serde` (config/state), `sqlx`/`rusqlite`
(persistence), `sha2` (checksums). GUI: Tauri (chosen at M2).
