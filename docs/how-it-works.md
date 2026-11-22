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
- [Scheduling and automation](#scheduling-and-automation)
- [Proxies](#proxies)
- [Missing and moved files](#missing-and-moved-files)
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
- **Size while waiting (GUI):** when a download is added and has to wait, the
  app asks the server for its size and file name with a small request (a
  `HEAD`, or if that is refused a one-byte range request), so the row shows
  them before it starts. Up to 4 of these run at once, apart from the queue,
  so they never take a download's slot or hold up adding. A server that
  refuses both or sends no size leaves the size blank, as before, and nothing
  reports an error. Once a download starts, its own first response is the
  authority and a late answer is ignored. The CLI doesn't do this: it has no
  queue.
- **Batches (GUI):** an add of two or more links with a name gives each
  download the same batch id (the id of the first) and the name, saved with
  the list. Each file is an ordinary download: it waits in the queue, takes
  the global limit and its own per-file limit; an optional batch limit is one
  limiter shared by the batch's files, so they stay under it combined (saved
  with each file, and rebuilt as one limiter on load). With a folder, the files go in
  `<destination>/<name>`; a folder that already exists, or holds another
  download's file, makes it `<name> (1)`, and files that clash inside it
  become `name (1).ext`. Retry failed for a batch restarts only that batch's
  failed downloads. Without a name the links are plain downloads.
- **Speed limits:** `--limit-rate` on the CLI; in the GUI, a global limit and
  an optional per-download limit, both changeable while a download runs. A
  download's limit is shared across all of its connections.
- **From the first byte:** a download that starts or resumes runs at its
  limit straight away, with no initial burst. Its speed is measured over the
  last 5 seconds from bytes that have passed the limit's wait, so it reads
  close to the limit, even when data arrives less often than that.

## Scheduling and automation

Rules for when downloads run, and what happens when they finish. The rules
are Manager state, evaluated against an injected clock (the tests run on a
fake one), and saved with the settings. The CLI has only the one-off forms
(`--at`, `--on-finish`); the rest is the desktop app's
[Schedules](gui.md#schedules).

- **Queue window (GUI):** a rule gives the queue hours of the day and days of
  the week, in the machine's local time; it may cross midnight (22:00 to
  06:00). Outside every enabled window nothing starts; downloads wait as
  queued, and the status bar says so once. When the window closes, running
  downloads pause with their `.part` and sidecar kept (not an error) and
  carry on from there when it opens, with no new download. Two exceptions:
  a download whose server can't resume is left to finish, since pausing would
  throw its bytes away, and a window set to *Finish downloads in progress*
  stops only new starts. A download you paused yourself is never resumed by a
  window. The window follows the wall clock, so on a night the clock changes
  it is an hour shorter or longer; a close inside the hour that repeats when
  the clock falls back is crossed once, not again. Rules are checked every
  second, apply at launch, and are saved with the settings. Rdm only
  schedules while it is running: it never wakes the machine or runs as a
  service.
- **Speed schedule (GUI):** a speed entry is a window and a limit. Inside the
  first enabled entry whose window is open, its limit replaces the global
  limit; outside every entry the global limit applies. The result is set on
  the one limiter all downloads share, on the same one-second check as the
  queue window and at launch, so a change lands mid-download without a
  restart. The global limit you set is kept as it is; a limit on a single
  download or batch still applies on top.
- **Scoped rules:** a queue window or speed entry can name a scope: three
  lists, of tags (ignoring case), hosts (the URL's host, exact, ignoring case,
  no port or wildcards) and batch ids. A download is in scope when it matches
  any name in every list that has names, so empty lists mean all downloads
  (the default; files saved before scopes load as this, and an older single
  tag, host or batch scope loads as a list of one). A scoped window is checked per download: a
  download is held, or paused at close, only by the enabled windows that match
  it, and starts if any of those is open or none match. The queue state the
  status bar shows covers only all-downloads windows. A scoped speed entry
  keeps one limiter per distinct scope (names compared ignoring case, order
  and repeats), shared by every matching download, set to the
  first open entry for that scope and unlimited outside; running downloads
  are handed it on the next one-second check, so tags added mid-download and
  rule changes apply live. A download in two scopes, or with its own, batch or
  global limit too, runs at the slowest. All-downloads entries still replace
  the global limit. A rule whose names match nothing is kept.
- **Start at:** a download can carry a local start time. Until it, it stays
  queued but is held out of the queue, so it uses no slot; the Manager checks
  the clock every second and then releases the download into the normal queue,
  under the usual cap. Everything given one start time together (one add, one
  batch, one selection) is one schedule. The time is saved with the download.
  One that passed while rdm was closed is not started at launch: it is marked
  missed and waits for the user, who can start it (the downloads join the
  queue oldest due first), give it a new time, or dismiss it, which leaves the
  downloads paused. The CLI's `--at` just waits in the foreground first.
- **Events and post-download actions:** the Manager announces two events on a
  broadcast channel, the way it does for missing files: a download finished,
  and the queue emptied (the last running or waiting download just completed
  or failed). The two post-download actions are subscribers; the download code
  doesn't know what they do. *Run a command* starts the program directly, with
  one argument per entry and `{download_path}` and `{download_name}` filled in, never through a
  shell; a failure to start or a non-zero exit is kept as a warning on that
  download (memory only) and doesn't touch its status. *Power action* arms
  when the queue empties and counts 60 seconds, checked every second, but
  only while nothing is running, waiting, held by the queue window or
  scheduled for later and no command it waits for is running (a command
  saved with `wait_for_exit` off, default on, never holds it); anything that shows up
  during the countdown drops it, and Cancel ends it. A power action goes
  through a backend trait: the GUI and CLI install the real one, a Manager
  nobody gave one refuses, and tests use a fake. Both actions are saved with
  the settings. In the CLI, `--on-finish` waits the same 60 seconds after the
  run, but only when every download succeeded.

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

## Missing and moved files

A finished download whose file is gone is flagged as missing; it stays
finished, and the flag is worked out from disk each time, never saved. The
check lists each folder once rather than looking up every file. It runs in
the background when the app starts, when its window regains focus (at most
once every 30 seconds) and just before a file is opened or shown, so a slow
or unplugged drive never freezes the list.

**Find file** looks for a missing download's file in three places, in order:
the folder it used to be in, the folders of your other downloads, and the
default download folder. It never searches a whole drive. Folders repeated
or inside another folder in the list are searched once, as part of the outer
one.

Each folder is scanned once into an index of file names and sizes, and every
missing download is then matched against that index, so finding a thousand
files is one pass, not a thousand. The scan is breadth first, so nearby files
come first and Cancel takes effect quickly. It has limits:

- Subfolders are searched three levels down.
- Up to 50,000 files and folders are looked at per search folder.
- Symbolic links and junctions are not followed, and hidden and system
  folders are skipped.

When a limit cuts a search short, the result says so, so the app can tell you
not every folder was fully searched.

A file is an exact match when its name and size are both the same. If the name
changed, it counts as a possible match only when it is the one file of that
size with the same extension. If several files share the size and the
download has a SHA-256 checksum, only those are hashed and the one with the
right digest is offered. A file that is another download's own file is never
offered.

Searching never relinks anything: it offers candidates, and you accept them.
Results are kept in memory until the download is relinked or its file
reappears; they are not saved. Each search is also written to the download's
log, with the folders searched and the outcome.

**Locate…** lets you pick any file by hand, but refuses one of a different
size. It uses the same name and size check as Find file.

## Engine and frontends

All download logic lives in the `rdm` engine library. The CLI and the GUI
(Tauri + React) are thin frontends over it: they start, pause, resume and
display downloads, but don't fetch anything themselves.
[The architecture diagram](architecture.html) shows the components and
follows a download through them.

What a pasted list of links means is decided in the engine too, so every
frontend reads a list the same way. Links split on whitespace and newlines,
and on a comma or pipe followed by another `http://` or `https://` link (a
comma inside one link stays put). Blank lines and `#` comments are ignored,
and only `http` and `https` links count. A link listed more than once is
added once, and a list of more than 5000 distinct links is refused with its
count.

A `[a-b]` range in a link's path or query expands to every value between
(`part[01-12].zip`, `file[a-f].txt`), keeping zero padding; the host never
expands. The number of links is worked out from the two ends, never by
generating them, so a huge range is refused instantly with its count (the
limit is 500 links per range). A range counting down, with mixed kinds of
ends, uneven padding, or a second range in the link is refused with its
reason, and the rest of the list is kept. Ranges expand before repeats are
removed. Only `[x-y]` of letters or digits counts as a range, so other
brackets, such as an IPv6 host's, are part of the link.

## A download's log on disk

Each download's Log tab keeps its last 200 informational lines (and up),
including Find file searches and relinks, across restarts. Every such line is
appended as it is logged, as one line of JSON, to `logs/<id>.jsonl` beside
`state.json`, so a crash loses at most the last line. Detailed (debug) lines
stay in memory only. The saved lines are read back the first time a
download's log is opened, and they hold exactly the redacted text the Log tab
shows. Removing a download from the list deletes its log file.

Logs are pruned once, in the background, at start: files not written for 90
days go, then files for downloads no longer in the list, then the least
recently written ones until all logs together take at most 20 MB. A file that
grows past 1000 lines is cut back to its last 200. If a log can't be written
or read, the download carries on, and one warning says the log won't survive a
restart.

## History on disk

What schedules, tags and the finish actions did is recorded by the engine in
`history.jsonl` beside `state.json`, one JSON event per line: the time, a kind
(window opened or closed, held, started, paused, missed, command, power, tag
added or removed), the download and the rule it concerns when there is one,
and a sentence. A queue window gets its own id when it is first saved, so its
events stay with it through edits (a copy is a new rule). The closing event
carries what finished while the window was open ("14 files, 12.3 GB"),
counted from downloads that complete inside it; the first look after start
only learns which windows are open, and records nothing.

Events are appended as they happen. At start, events older than 90 days and
ones for downloads no longer in the list are dropped, then the oldest until
the file takes at most 20 MB; the file is cut the same way if it grows past
that while rdm runs. Removing a download deletes its events. The Log tab and a
rule's History read the events from the engine; the frontend records nothing.
If the file can't be written, the events are still kept in memory for the
session.

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
