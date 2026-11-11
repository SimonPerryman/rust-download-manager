# Changelog

All notable changes to rdm, the command-line tool and the desktop app, are
listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and rdm uses [Semantic Versioning](https://semver.org/). The CLI and the
desktop app share one version.

## Unreleased

### Added

- In-app updates (desktop app): a one-time question asks whether to check
  for updates automatically (on start and daily); Settings has the version
  and Check for updates. A newer version shows a bar with What's new,
  **Restart now** (downloads pause, as on a quit), **Install when I quit**
  and **Later**; after an update restart, **Resume all** resumes exactly the
  downloads it paused. Updates are signed and checked before install. `.deb`
  installs get a link to the new `.deb` instead. Copies installed from 0.2.0
  or earlier need one manual update to get the updater.
- The desktop app's log file starts each run with the version, OS, WebView,
  memory use and the settings that matter (no URLs, paths or tags), and
  records every panic with its backtrace, flushed to disk at once. After a
  warning or error the file is flushed to disk, at most once every 5
  seconds.
- The CLI's `-v` output starts with the version and OS, and a panic ends with
  where to report the bug (exit code 101).
- Settings → Troubleshooting (desktop app): **Open logs folder**, and **Copy
  diagnostics**, which copies a report for a bug report (version, OS,
  settings and the last 300 log lines). It hides links' sites and paths,
  the file names and folders of the downloads in your list, and your home
  folder.
- If the desktop app's window fails to draw, it shows "Something went
  wrong" with Reload, Open logs folder and Copy diagnostics instead of a
  blank window, and the error goes into the log, as does any other error
  the window doesn't handle.
- The Log tab's **Save…** saves a download's lines as a text file.
- A bug report form on GitHub.

### Changed

- Redacted links in logs and error messages now show that a login was used,
  as `<redacted>:<redacted>@` (or `<redacted>@` for a user name alone),
  instead of dropping it; the password and user name stay hidden.
- The Log tab's **Copy** copies every line rdm keeps for the download,
  detail included, whichever view is showing, and **Detailed** is now a
  **Simple | Detailed** switch.
- Settings (desktop app): **Ask before removing a download for good** moved
  from Display to Downloads, and "Max concurrent downloads" is now
  **Concurrent downloads**.
- A download with its own speed limit (desktop app) reads its speed against
  the limit in the Speed column, such as "49.8 / 50 KB/s" ("≤ 50 KB/s" while
  paused or waiting), instead of showing a gauge icon there. The row's speed
  limit button turns orange while a limit is set.
- The sidecar's temporary save file is now hidden (`.<file>.rdm.<pid>.<n>.tmp`,
  also marked hidden on Windows), so it no longer flickers in file managers
  beside the download.

### Fixed

- A bug that panics inside a download now fails just that download, with
  "internal error, please report it", instead of leaving it stuck as
  downloading and holding a queue slot until restart.
- A desktop-app action whose command panics now shows an error instead of
  closing the app or never finishing.
- The desktop app's log no longer drops lines under a burst or loses its last
  lines when the app closes.
- The desktop app's log file no longer has terminal colour codes around a
  download's fields (seen on Linux).
- A speed-limited download's speed no longer reads blank or 0 when it
  starts, resumes or receives data in slow bursts, and no longer overshoots
  its limit early on: the limit applies from the first byte, with no initial
  burst, and the speed is measured after the limit's wait.
- Time left ("1 h 13 min") and, in a narrow window, the size ("346.2 MB") are
  no longer cut off.

## 0.2.0 - 2026-11-10

### Added

- Archive (desktop app): **Archive finished** (every completed and stopped
  download) and a row's Archive move finished downloads out of the working
  views into **Archived**, with Undo. From Archived, Unarchive moves them
  back and Remove drops them for good (the file stays on disk; a failed
  download's partial file is deleted), after asking first unless you've told
  it not to.
- Tags (desktop app): label downloads in the Add dialog, from Tags… in the
  menu or in the details pane, and filter by tag from the sidebar, with
  counts. In Archived, the tags listed are the archived downloads'.
- The Add dialog notes a link that's already in your archive, and still adds
  it.
- The Log tab's **Copy** button copies the lines shown, each with its time.

### Changed

- **Clear finished** is now **Archive finished**: rows go to Archived instead
  of being deleted, and removing a download for good happens from Archived.
- The desktop app's download list (`state.json`) moves to a new format that
  can hold archived and tagged downloads. A list from 0.1.0 opens unchanged.
  A list saved by a newer rdm is now refused in a dialog naming its version,
  and rdm closes, rather than opening it and saving it back without what this
  version doesn't understand.
- Going back to 0.1.0 after this version opens the list, but 0.1.0's first
  save drops archive flags and tags.
- The details pane draws one bar per open connection (lane) instead of one
  line per piece, headed with how many lanes are open. Bars appear as lanes
  open and merge back as they close, each showing only its own lane's work,
  so nothing is drawn twice; hover one for its lane and piece. A completed
  download shows one finish bar and a line such as "Downloaded in 4 pieces ·
  up to 3 lanes".
- "Completed" replaces "Done" for a download that worked: the sidebar
  filter, the row's status, the details pane and the finish notification
  ("Download complete"), and the log's summary line ("Completed · 4.00 MB in
  10s · …"), in the Log tab and the CLI's `-v` output. "Finished" still means
  any ending, as in **Archive finished**.
- When rdm closes a connection because more weren't helping, it closes the
  highest-numbered lane, so Lane 1 usually stays. If that lane is stuck, the
  next free one closes after 3 seconds instead.
- Clearer log lines: "Added Lane 2 — now 2 lanes", "Closing Lane 3 — more
  lanes didn't speed this up", and in the detail, which piece each lane is on
  and whose work it took over, with sizes rather than byte offsets.

## 0.1.0 - 2026-11-09

The first release: everything below is new.

### Downloads

- Download a file over HTTP or HTTPS with `rdm <url>`, or from the desktop
  app.
- HTTPS trusts the certificates your operating system trusts, so downloads
  work on networks that use a company or other custom certificate authority.
  On Linux this needs the system's CA certificates installed (the
  `ca-certificates` package), as most distributions do by default.
- Large files download faster: each one is split across several connections
  (4 by default, up to 16). rdm starts on one connection and adds more only
  while the speed keeps rising.
- Never start over: an interrupted download (a crash, a restart, a dropped
  connection or Ctrl-C) resumes where it stopped. rdm checks that the file
  on the server hasn't changed before it continues.
- Dropped connections and server errors are retried with a back-off, and a
  server's rate-limit response is respected.
- Optional SHA-256 checksum: the file is checked before it's saved under its
  final name.
- Speed limits, per download and in total.
- Safe file names: a download stays in the folder you chose and never
  overwrites another file.

### Command-line tool

- `-o` picks the output path, `-c` the maximum number of connections,
  `--limit-rate` caps the speed and `--checksum sha256:<hex>` verifies the
  result.
- A progress bar with speed and time left; Ctrl-C pauses, and running the
  same command again resumes.
- Clear exit codes for success, failure, invalid arguments and a pause.

### Desktop app

- One sortable table of every download, kept across restarts, with light and
  dark themes.
- Add many links at once (one per line), by pasting or by dragging a link onto
  the window, with a preview of each file before anything starts.
- A queue: downloads beyond the concurrency limit wait their turn, and you can
  reorder them.
- Pause, resume, stop, retry and remove, for one download or a selection.
- Live connection lanes and a speed graph of the last minute in the details
  pane.
- A download's log reads as a short story, one line per event: how the
  download started, lanes added or lost and reconnected, pauses and where it
  resumed, rate-limit waits, the checksum result, where the file was saved,
  and a closing summary with the time taken, average and peak speed and
  retries. Tick **Detailed** to also see each request. `rdm -v` prints the
  same lines.
- Filter by status or folder, search by name, a right-click menu and full
  keyboard control.
- Settings for the theme, concurrent downloads, the global speed limit and
  connections per download.
