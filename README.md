# rdm

A download manager written in Rust, with a command-line tool and a desktop GUI
built on the same download engine.

![The rdm desktop app: a sidebar of status and folder filters, and a table of downloads that are done, downloading, waiting, paused and failed](docs/screenshots/main-light.png)

<sub>A dark theme is included too: [screenshot](docs/screenshots/main-dark.png).</sub>

## Features

- **Parallel segmented downloads.** A large file (2 MiB or more) on a server
  that supports range requests is split into byte ranges fetched over up to 4
  connections by default (1–16 with `-c`). Connections are added while the
  measured speed keeps rising, and an idle connection takes over half of the
  largest remaining range. Servers without range support fall back to one
  connection automatically.
- **Resume after a crash or restart.** Data streams to `<file>.part`, with a
  `<file>.rdm` sidecar that records progress. Running the same command again
  continues where it stopped, re-fetching only the missing ranges, and only if
  the server's `ETag` or `Last-Modified` still matches.
- **Pause, resume and stop.** Ctrl-C pauses a CLI download. In the GUI you can
  pause, resume or stop any download, and choose whether a stop keeps the
  partial file.
- **Speed limits.** `--limit-rate` on the CLI. In the GUI, a global limit and
  an optional per-download limit, both changeable while a download runs.
- **A download queue (GUI).** At most 3 downloads run at once by default
  (configurable). The rest wait and start in order as slots free up.
- **Polite retries.** Timeouts, dropped connections and 5xx/408/429 responses
  are retried with exponential backoff. A host that rate-limits (429/503) is
  backed off from rather than sent more connections, and its `Retry-After`
  (up to 60 s) is honoured.
- **Integrity checks.** The finished file's size is always checked, and an
  optional SHA-256 checksum (`--checksum sha256:<hex>`) is verified before the
  file is moved into place.
- **Safe file names.** A name taken from the server or the URL is made safe on
  every OS and always lands inside the download folder. An existing file is
  never overwritten; the new one is saved as `name (1).ext`.
- **CLI and GUI.** The `rdm` CLI needs no webview or Node.js. The GUI (Tauri +
  React) shows every download in one sortable table, keeps the list across
  restarts, and has light and dark themes.
- **See each connection at work (GUI).** A multi-connection download draws its
  pieces as lanes, one per range of the file. A details pane shows the lanes
  full size, a graph of the last minute's speed, and the checksum, with a
  button to verify a finished file against a SHA-256 you paste.
- **Built for many downloads at once (GUI).** Paste a list of links to add
  them in one go, select several rows to act on together, right-click a row
  for its actions, reorder the queue by dragging, and drive the table from the
  keyboard.

## Install

Prebuilt binaries aren't available yet. They're planned for the next
milestone, [M6](milestones/milestone-06-releases-install.md), and will be
published on
[GitHub Releases](https://github.com/SimonPerryman/rust-download-manager/releases).
Until then, build from source.

### CLI

You need [Rust](https://www.rust-lang.org/tools/install) (stable; the MSVC
toolchain on Windows, which needs the Visual Studio C++ Build Tools).

```sh
cargo install --locked --git https://github.com/SimonPerryman/rust-download-manager rdm
```

This builds the `rdm` binary and puts it in `~/.cargo/bin`. The trailing `rdm`
picks the CLI package out of the repository's workspace. From a clone,
`cargo install --locked --path .` does the same.

### Desktop GUI

The GUI lives in [`gui/`](gui/). You need:

- Rust (as above);
- [Node.js](https://nodejs.org/) and [pnpm](https://pnpm.io/);
- the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your
  OS. On Windows that's the Visual Studio C++ Build Tools and the WebView2
  runtime, which ships with Windows 11.

```sh
git clone https://github.com/SimonPerryman/rust-download-manager.git
cd rust-download-manager/gui
pnpm install
pnpm tauri build    # release build; the app and installers land in ../target/release/
```

On Windows, run the build from a Developer PowerShell for VS so the MSVC
linker is on `PATH`.

## Quick start

```sh
# Download into the current directory (the file name comes from the server):
rdm https://example.com/big-file.zip

# Choose where to save it:
rdm https://example.com/big-file.zip -o ~/Downloads/archive.zip

# Use up to 8 connections (-c 1 for a single connection):
rdm https://example.com/big-file.zip -c 8

# Cap the speed at 2 MiB/s:
rdm https://example.com/big-file.zip --limit-rate 2M

# Verify the result against a SHA-256 checksum:
rdm https://example.com/big-file.zip --checksum sha256:<hex>
```

Interrupted, or pressed Ctrl-C? Run the same command again to resume. Ctrl-C
saves progress, prints `Paused — run the same command again to resume`, and
exits with code 130; a second Ctrl-C quits at once. To discard a partial
download instead, delete its `<file>.part` and `<file>.rdm`.

To start the GUI, run the app that `pnpm tauri build` produced, or
`pnpm tauri dev` from `gui/` during development.

## CLI reference

```
rdm [OPTIONS] <URL>
```

| Argument | Description |
|---|---|
| `<url>` | The URL to download (required). |
| `-o, --output <path>` | Output file path. Defaults to a name derived from the URL or headers, saved in the current directory. |
| `--checksum <algo:hex>` | Verify the finished file against a checksum, e.g. `sha256:…`. |
| `-c, --connections <n>` | Max parallel connections, 1–16 (default 4). Starts on one and adds more, up to `n`, while measured speed keeps rising. Used only for files of 2 MiB or more on servers that support range requests. |
| `--limit-rate <rate>` | Cap the download speed in bytes/s, with an optional `K`/`M`/`G` suffix (×1024), e.g. `500K` or `2M`. Shared across all of the download's connections. |
| `-v, --verbose` | Also log informational engine events (download start, fallbacks) to stderr. |
| `-h, --help` / `-V, --version` | Print help or the version. |

Retries and warnings print to stderr. A failed download is reported once, as
`Error: failed to download …`. `RUST_LOG` (e.g. `RUST_LOG=debug`) overrides
the log level.

## Using the GUI

- **Add** downloads with the **Add** button, by dragging a link onto the
  window, or by pressing Ctrl/Cmd+V (outside a text box) with one or more
  links on the clipboard. The dialog takes one link per line and previews each
  file's name and site, skipped lines and links already in your list before
  anything starts ("Add 3 downloads"); Ctrl+Enter adds. The folder starts at
  the one you used last. Under **More options** you can give a SHA-256
  checksum and a speed limit.
- **Filter** with the sidebar (by status, or by destination folder) or by name
  from the toolbar. Click a column header to sort. The sidebar also shows the
  total speed with a graph of the last minute. In a window narrower than about
  760 px the sidebar shrinks to an icon rail (hover an icon for its name) and
  less important columns hide.
- **Select** a row by clicking it; Ctrl+click adds or removes rows and
  Shift+click selects a range. With two or more selected, the toolbar offers
  the actions that apply to them all (Open, Pause, Resume, Stop, Retry, Remove
  from list).
- **Right-click** a row (or press Shift+F10 or the menu key) for every action
  that fits it, such as Show in folder, Copy link, Download again, Delete
  partial file and Remove from list.
- **Keyboard:** with the table focused, ↑/↓ move (Shift extends the
  selection), Home/End jump, Ctrl+A selects all, Space pauses or resumes,
  Enter opens a finished file, Delete removes from the list and Escape clears
  the selection.
- **Queue order:** waiting downloads start in order, and the first says **Next
  up**. Drag a waiting row by its handle, press Alt+↑/Alt+↓, or use **Start
  next**, **Move up** and **Move down** from its menu or the details pane.
- **Density:** the toolbar toggles compact 32 px rows (the default) and
  comfortable 48 px rows that also show each file's site.
- **Lanes:** a multi-connection download draws one line per piece of the file
  in the Progress column, showing where the piece sits and how far it has got.
- **Details:** click a row to open the details pane under the table; it then
  follows the row you select (drag its top edge to resize). **Overview** shows sizes, speed, time left, the lanes
  full size (hover one for details), the link, the folder and the checksum. A
  finished download gets a summary such as "5.8 GB in 2 min 14 s, average
  44 MB/s, checksum verified" and **Verify checksum**. **Speed** graphs the
  last 60 seconds, with the speed limit as a dashed line. A finished
  download's lanes are kept only until the app closes; after a restart the
  pane shows the summary alone.
- **Finished** downloads stay where they are, marked Done, with **Open** and
  **Show in folder**. **Clear finished** removes Done and Stopped rows from the
  list (with Undo); downloaded files stay on disk.
- **Failed** downloads are pinned to the top with the reason in plain words and
  a **Retry** button.
- **Settings** (the sliders button) holds the theme, the max number of
  concurrent downloads, the global speed limit and the max connections per
  download. The status bar shows the total speed, and you can click the global
  limit there to change it.

![The details pane's Speed tab for a running download: speed now, average and peak, and a graph of the last minute, under a table whose rows draw their lanes](docs/screenshots/details.png)

The download list and settings are stored in `~/.rdm/state.json`. The GUI also
writes a daily log in `~/.rdm` (`rdm.YYYY-MM-DD.log`, the last 7 kept). Logged
URLs and error messages omit any `user:password@`, query string or fragment, so
they're safer to paste into a bug report. `RDM_HOME` relocates `~/.rdm`, and
`RDM_DOWNLOAD_DIR` changes the Add dialog's default folder.

## Building and testing from source

The repository is a Cargo workspace: the `rdm` engine library and CLI at the
root, and the GUI's Rust crate (`rdm-gui`) under `gui/src-tauri`. All download
logic lives in the engine; the CLI and the GUI are thin frontends over it.

```sh
cargo build                  # the engine and CLI
cargo test                   # fast, offline test suite
cargo test -- --ignored      # also run the live network smoke tests
cargo clippy -- -D warnings  # lint
cargo fmt --all --check      # format check

cd gui
pnpm test                    # frontend unit tests
pnpm lint                    # frontend lint
```

The default tests run offline against a local HTTP server. The GUI also has an
end-to-end suite that drives the real app with WebdriverIO; see the
[`gui/e2e` package](gui/e2e/package.json) (`pnpm e2e`), which needs
`tauri-driver` and, on Windows, Microsoft Edge WebDriver. `pnpm screenshots` in
the same package regenerates the images in [`docs/screenshots`](docs/screenshots/).

Contributor conventions are in [AGENTS.md](AGENTS.md).

## Project status

Early, but usable. Work is planned in milestones; see the [roadmap](roadmap.md)
and the tickets in [`milestones/`](milestones/).

- **Done:** M0 walking skeleton; M1 robust single-connection downloads
  (resume, retries, checksums); M2 desktop GUI; M3 multi-segment parallel
  downloads; M4 queue, speed limits and connection ramping.
- **In progress:** M5, a GUI redesign. The new layout (sidebar, table, status
  bar, stop and clear-finished flows), the details pane, lanes and speed
  graphs, batch add, the row menu, multi-select, keyboard shortcuts and queue
  reorder have shipped. What remains is a per-download Log tab in the details
  pane.
- **Next:** M6, prebuilt binaries and installers on GitHub Releases.

CI builds and tests on Linux and Windows. macOS isn't tested yet.

## Security

Please report vulnerabilities privately through GitHub's
[Report a vulnerability](https://github.com/SimonPerryman/rust-download-manager/security/advisories/new)
form, not in a public issue.

## Licence

Dual-licensed under either [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option.

Bundled third-party icons and fonts are credited in
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
