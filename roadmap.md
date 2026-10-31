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
| M0 | Walking skeleton | todo | [[milestone-00-walking-skeleton]] |
| M1 | Robust single-connection (MVP core) | todo | [[milestone-01-robust-single-connection]] |
| M2 | GUI (first post-MVP) | todo | [[milestone-02-gui]] |
| M3 | Multi-segment parallel downloads | todo | [[milestone-03-multi-segment]] |
| M4 | Queue & throughput management | todo | [[milestone-04-queue-throughput]] |

**MVP = M0 + M1** — lean, single-connection, CLI-only. Kill-and-resume works.

Dependency notes: M2, M3 and M4 all depend only on M1, not on each other, so
their order is flexible. M2 (GUI) is sequenced first post-MVP by choice; M3 is
the parallel-download speed feature; M4 is the day-to-day queue/throttle layer.

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

## Candidate crates

`reqwest` (HTTP), `tokio` (async), `futures` (concurrency), `clap` (CLI),
`indicatif` (CLI progress), `serde` (config/state), `sqlx`/`rusqlite`
(persistence), `sha2` (checksums). GUI toolkit TBD at M2.
