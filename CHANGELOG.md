# Changelog

All notable changes to rdm, the command-line tool and the desktop app, are
listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and rdm uses [Semantic Versioning](https://semver.org/). The CLI and the
desktop app share one version.

## Unreleased

The first release: everything below is new.

### Downloads

- Download a file over HTTP or HTTPS with `rdm <url>`, or from the desktop
  app.
- HTTPS trusts the certificates your operating system trusts, so downloads
  work on networks that use a company or other custom certificate authority.
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
- Live connection lanes, a speed graph of the last minute, and a per-download
  log in the details pane.
- Filter by status or folder, search by name, a right-click menu and full
  keyboard control.
- Settings for the theme, concurrent downloads, the global speed limit and
  connections per download.
