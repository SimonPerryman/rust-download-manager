# Changelog

All notable changes to rdm, the command-line tool and the desktop app, are
listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and rdm uses [Semantic Versioning](https://semver.org/). The CLI and the
desktop app share one version.

## Unreleased

### Added

- Archive (desktop app): **Archive finished** and a row's Archive move
  finished downloads out of the working views into **Archived**, with Undo.
  From Archived, Unarchive moves them back and Remove drops them for good
  (the file stays on disk), after asking first unless you've told it not to.
- Tags (desktop app): label downloads in the Add dialog, from Tags… in the
  menu or in the details pane, and filter by tag from the sidebar, with
  counts. In Archived, the tags listed are the archived downloads'.
- The Add dialog notes a link that's already in your archive, and still adds
  it.

### Changed

- **Clear finished** is now **Archive finished**: rows go to Archived instead
  of being deleted, and removing a download for good happens from Archived.

- The desktop app's download list (`state.json`) moves to a new format that
  can hold archived and tagged downloads. A list from 0.1.0 opens unchanged.
  A list saved by a newer rdm is now refused with a message naming its
  version, rather than opened and saved back without what this version
  doesn't understand.

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
