# How it works

What rdm's download engine does under the hood. The CLI and the desktop app
share it, so everything here applies to both.

## Contents

- [Parallel segmented downloads](#parallel-segmented-downloads)
- [Resume](#resume)
- [Retries and backoff](#retries-and-backoff)
- [Integrity checks](#integrity-checks)
- [Safe file names](#safe-file-names)
- [Marked as downloaded (Windows)](#marked-as-downloaded-windows)
- [The queue and speed limits](#the-queue-and-speed-limits)
- [Proxies](#proxies)
- [Engine and frontends](#engine-and-frontends)
- [When something goes wrong](#when-something-goes-wrong)

## Parallel segmented downloads

A large file (2 MiB or more) on a server that supports range requests is split
into byte ranges fetched in parallel, over up to 4 connections by default
(1–16 with `-c`, or the GUI's max connections setting).

- **Ramping:** the number of connections keeps adjusting for the whole
  download, judged by the speed measured each second:
  - *Ramp up:* a download starts on one connection and adds one at a time,
    up to the maximum, while each makes it more than 10% faster.
  - *Settle:* when one more connection stops helping, it closes that one
    again and settles.
  - *Re-probe:* a settled download below the maximum tries one more
    connection every 30 seconds, and when its speed limit is raised or
    removed (once it has measured a second at the new limit), in case it
    can now go faster. If that makes it more than 10% faster than its
    recent speed, the connection stays and ramping carries on; if not, or
    if the speed limit still holds it back, it closes again. A connection
    still being tried when the limit is raised closes, to be tried again
    at the new limit (logged as "its speed limit changed mid-try"). A
    server that refused a connection with a 503 isn't offered more than
    it accepted until the download next has to retry; one that answered
    429 gets the rest of the download in one request instead (see
    [Retries and backoff](#retries-and-backoff)).
  - *Shed:* a settled download whose speed stays below 70% of its best
    (smoothed over a few seconds) for 3 measured seconds running closes one
    connection (never the last), since the extra connections are likely
    hurting rather than helping. A second slowed by rdm's own speed limit
    doesn't count. Its best becomes the current speed, so a later re-probe
    can win the connection back once the speed recovers.

  A second spent waiting on rdm's own speed limit, in which a connection
  opened or closed, or in which the work was running out, says nothing
  about the server, so it isn't measured. Nor is one while a connection is
  still waiting on its first bytes, so a server that's slow to connect
  doesn't read as "more connections don't help"; one still silent after 3
  seconds counts as stuck, and is measured like any other.
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

Each save of the sidecar writes a hidden temporary file beside it,
`.<file>.rdm.<pid>.<n>.tmp` (also marked hidden on Windows), then renames it
over the sidecar, so a crash mid-save never leaves a half-written sidecar. A
temporary file left by a crash is deleted when the download next starts,
finishes, or has its partial file thrown away.

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

Some hosts limit how often you may ask, not how many connections you hold:
every new request past a few can cost another `Retry-After`. So once a host
answers 429, or refuses with a `Retry-After`, rdm fetches everything left in
one request on one lane, from the first missing byte to the last, reading
past any parts already on disk in between. The rest of the download then
normally costs just that one wait; a connection that drops mid-way can cost
another. The trade-off is the parts read past: they are fetched again (and
count against a speed limit), so a host that refuses late in a download
with every lane part-way through re-sends more. This lasts until the
download stops; resuming it starts on several lanes again.

## Integrity checks

The finished file's size is always checked. An optional SHA-256 checksum
(`--checksum sha256:<hex>` on the CLI, or **More options** in the GUI's Add
dialog) is verified before the file is moved into place.

## Safe file names

A name taken from the server or the URL is made safe on every OS and always
lands inside the download folder. An existing file is never overwritten; the
new one is saved as `name (1).ext`.

## Marked as downloaded (Windows)

On Windows, every finished file is tagged as downloaded from the internet
(Mark-of-the-Web), just as a browser tags its downloads, so SmartScreen,
Office's Protected View and antivirus give it the same extra checks. The tag
records the download's URL without any login or query string, since those
often carry tokens. A drive that can't hold the tag (FAT32, exFAT, some
network shares) leaves the file unmarked, and the download still succeeds.

## The queue and speed limits

- **Queue (GUI):** at most 3 downloads run at once by default (configurable).
  The rest wait and start in order as slots free up.
- **Speed limits:** `--limit-rate` on the CLI; in the GUI, a global limit and
  an optional per-download limit, both changeable while a download runs. A
  download's limit is shared across all of its connections.
- **From the first byte:** a download that starts or resumes runs at its
  limit straight away, with no initial burst. Its speed is measured over the
  last 5 seconds from bytes that have passed the limit's wait, so it reads
  close to the limit, even when data arrives less often than that.

## Proxies

A download uses one of three proxy modes, and every request it makes,
each connection of a segmented download included, goes the same way:

- **System** (the default): the `HTTP_PROXY`, `HTTPS_PROXY` or `ALL_PROXY`
  environment variable, with `NO_PROXY`'s hosts going direct; when none is
  set, on Windows and macOS the operating system's proxy settings and their
  bypass list. Proxy auto-config (PAC scripts, WPAD) isn't supported. The
  system proxy is read as each download starts or resumes.
- **None:** always connect directly.
- **Custom:** an HTTP, HTTPS or SOCKS5 proxy you give (`socks5h` to have
  the proxy resolve host names too), for every host.

The CLI picks the mode with `--proxy` and `--no-proxy`, the desktop app
with its Proxy setting, where a change applies from each download's next
start or resume. Logs and error messages show a custom proxy with its login
redacted, like any URL's.

## Engine and frontends

All download logic lives in the `rdm` engine library. The CLI and the GUI
(Tauri + React) are thin frontends over it: they start, pause, resume and
display downloads, but don't fetch anything themselves.

## When something goes wrong

- **A bug in a download:** if a download's task panics (a bug in rdm), only
  that download fails. It shows "internal error, please report it" with the
  panic's message, its Log tab has the same line, its queue slot frees up and
  the next download starts. The rest of the app keeps working, even if the
  panic happened mid-update.
- **A bug in a desktop-app command:** a button whose command panics shows an
  error, as any failed action does, and the app keeps running.
- **A bug in the desktop app's window:** an error while drawing the window
  shows a "Something went wrong" screen with Reload instead of a blank
  window, while downloads carry on. That error, and any other the window
  doesn't handle, goes into the log file (at most 20 a minute, so one that
  repeats can't flood it).
- **The log file (desktop app):** lines are written to the file as they're
  logged, from a background thread, so a crash loses at most the few lines
  still queued. A burst waits rather than dropping lines. After a warning or
  an error the file is flushed to disk, at most once every 5 seconds (one
  held back by that limit happens with the first line logged after those 5
  seconds, or when the app closes),
  so a power cut is less likely to take recent errors with it. Every panic, on any thread, is
  appended to the file at once with its backtrace and flushed to disk before
  anything else happens; the same goes for the startup lines (version, OS,
  WebView, memory use and settings). Closing the app writes out every queued
  line.
- **Privacy:** rdm sends nothing anywhere: no telemetry, no crash reports.
  Logs stay on your disk, and the URLs they mention are redacted: a login
  shows only as `<redacted>:<redacted>@` (or `<redacted>@`), a query string
  as `?<redacted>`, and a fragment is dropped, while the site name and path
  stay. The desktop app's Copy diagnostics only puts a report on your
  clipboard, for you to read and paste into a bug report yourself, and
  hides more: links keep only their scheme (sites and paths become
  `<site-1>` and `<path>`), and the file names and folders of the
  downloads in your list become `<file-1>.iso` and `<folder-1>`.
