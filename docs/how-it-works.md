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
  maximum, while the measured speed keeps rising.
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
