# How it works

What rdm's download engine does under the hood. The CLI and the desktop app
share it, so everything here applies to both.

## Contents

- [Parallel segmented downloads](#parallel-segmented-downloads)
- [Resume](#resume)
- [Retries and backoff](#retries-and-backoff)
- [Integrity checks](#integrity-checks)
- [Safe file names](#safe-file-names)
- [The queue and speed limits](#the-queue-and-speed-limits)
- [Engine and frontends](#engine-and-frontends)

## Parallel segmented downloads

A large file (2 MiB or more) on a server that supports range requests is split
into byte ranges fetched in parallel, over up to 4 connections by default
(1–16 with `-c`, or the GUI's max connections setting).

- **Ramping:** a download starts on one connection and adds more, up to the
  maximum, while the measured speed keeps rising. If more connections stop
  helping, it closes one again.
- **Which lane closes:** connections are numbered Lane 1, Lane 2… in the
  order they open, and the highest-numbered one closes first, so Lane 1
  usually stays for the whole download and the numbers stay compact. A lane
  closes between two reads, putting the rest of its piece back for another
  lane. If the highest lane is stuck (say, waiting on a slow server) and
  hasn't closed within 3 seconds, the next lane that's free to stop closes
  instead, so a stuck lane can't keep more connections open than the
  download should have; Lane 1 stands in only when it is the one other lane.
  3 seconds is three of the ramp's 1-second measuring windows: long enough
  that a lane mid-read gets its turn, short enough that a slow host isn't
  kept busy for long.
- **Work-stealing:** a connection that finishes its range takes over half of
  the largest remaining range, so the last few seconds don't hang on one slow
  connection.
- **Fallback:** a server without range support, or a smaller file, is fetched
  over one connection automatically.

## Resume

Data streams to `<file>.part`, with a `<file>.rdm` sidecar that records
progress. Running the same command again (or resuming in the GUI, even after a
restart) continues where it stopped, re-fetching only the missing ranges, and
only if the server's `ETag` or `Last-Modified` still matches.

Quitting the GUI pauses what's running, keeping each partial file, and
nothing starts again on its own at the next launch. Restarting the GUI into
an update pauses downloads the same way and also marks the ones that were
running or waiting, so after the restart **Resume all** resumes exactly
those; resuming one by hand, or dismissing the offer, clears its mark.

## Retries and backoff

Timeouts, dropped connections and 5xx/408/429 responses are retried with
exponential backoff. A host that rate-limits (429/503) is backed off from
rather than sent more connections, and its `Retry-After` (up to 60 s) is
honoured.

## Integrity checks

The finished file's size is always checked. An optional SHA-256 checksum
(`--checksum sha256:<hex>` on the CLI, or **More options** in the GUI's Add
dialog) is verified before the file is moved into place.

## Safe file names

A name taken from the server or the URL is made safe on every OS and always
lands inside the download folder. An existing file is never overwritten; the
new one is saved as `name (1).ext`.

## The queue and speed limits

- **Queue (GUI):** at most 3 downloads run at once by default (configurable).
  The rest wait and start in order as slots free up.
- **Speed limits:** `--limit-rate` on the CLI; in the GUI, a global limit and
  an optional per-download limit, both changeable while a download runs. A
  download's limit is shared across all of its connections.

## Engine and frontends

All download logic lives in the `rdm` engine library. The CLI and the GUI
(Tauri + React) are thin frontends over it: they start, pause, resume and
display downloads, but don't fetch anything themselves.
