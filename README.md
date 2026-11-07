# rdm

A download manager in Rust. `rdm` downloads files from a URL — on the command
line or through a desktop GUI — streaming to disk with a live progress readout.

> **Status: early.** M0 + M1 are a robust single-connection CLI download
> (pause/resume, resume-after-restart, retry, integrity checking); M2 adds a
> desktop GUI over the same engine; M3 adds multi-segment parallel downloads;
> M4 adds a download queue, speed limits (CLI `--limit-rate`, and global +
> per-download limits in the GUI) and throughput-driven connection ramping.

## Features

- Download over HTTP/HTTPS, streamed to disk chunk-by-chunk (no full buffering).
- Parallel segments: a large file (≥ 2 MiB) on a server that supports range
  requests, sends a strong `ETag` or a `Last-Modified`, and doesn't set a
  `Content-Encoding` is split into byte ranges fetched in parallel over up to
  N connections (4 by default, `-c` to change), added while measured speed
  keeps rising; idle connections take over half of the largest remaining
  range. Servers without range support fall back to one connection
  automatically.
- Follows redirects and handles common HTTP status codes.
- Derives the output filename from `Content-Disposition` or the URL, or takes
  an explicit path.
- Live progress bar with size, speed, ETA, and the live connection count
  (`N conn` while more than one is open).
- Resumable: data streams to a `<file>.part` with a `<file>.rdm` sidecar;
  re-running the same command continues where an interrupted download left off
  (via HTTP range requests, validated against `ETag`/`Last-Modified`). A
  multi-segment download re-fetches only the ranges that were still missing.
- Retries transient failures (timeouts, dropped connections, 5xx/408/429) with
  exponential backoff. A rate-limited host (429/503) is backed off from, never
  hammered with new connections, and its `Retry-After` (up to 60 s) is
  honoured. A request that can't connect within 20 s, gets no response within
  30 s, or whose download goes 30 s without receiving data is dropped and
  retried; time spent under your own speed limit doesn't count.
- Verifies the finished file's size, and an optional `--checksum sha256:<hex>`,
  before moving it into place; never clobbers an existing file (`name (1).ext`).
- Speed limits: `--limit-rate` on the CLI; in the GUI, a global limit and an
  optional per-download limit, set when adding or changed from the download's
  row (both applied live).
- Download queue (GUI): at most 3 downloads stream at once by default
  (configurable); the rest wait as *Queued* and start in order as slots free.
- Optional **desktop GUI** (Tauri + React): manage many downloads at once —
  add/pause/resume/cancel, live progress/speed/ETA (plus *N connections* while
  more than one is open), a download queue with a
  concurrency cap, global and per-download speed limits, and an Active/History
  list that persists across restarts. The GUI is an *additive* frontend; the CLI and
  engine stay fully functional headless (no webview/Node needed).

## Installation

`rdm` is built from source with [Rust](https://www.rust-lang.org/tools/install)
(stable; MSVC toolchain on Windows).

```sh
# Clone, then install the binary onto your PATH:
cargo install --path .

# …or just build it (binary lands in target/release/):
cargo build --release
```

## Usage

```sh
# Download into the current directory (filename derived from the server):
rdm https://example.com/big-file.zip

# Choose the output path:
rdm https://example.com/big-file.zip -o ~/Downloads/archive.zip

# Verify the download against a SHA-256 checksum:
rdm https://example.com/big-file.zip --checksum sha256:<hex>

# Use up to 8 connections, added while measured speed keeps rising (-c 1 for one):
rdm https://example.com/big-file.zip -c 8

# Cap the speed at 2 MiB/s:
rdm https://example.com/big-file.zip --limit-rate 2M

# Interrupted? Re-run the same command to resume from where it stopped.
```

Ctrl-C pauses: rdm saves its progress, prints `Paused — run the same command
again to resume`, and exits with code 130; a second Ctrl-C quits at once. The
partial download stays on disk as `<file>.part` plus a `<file>.rdm` sidecar; to
discard it instead of resuming, delete both files. With
`--checksum`, the final line says the checksum was verified, e.g.
`Downloaded 1048576 bytes to archive.zip (sha256 verified)`.

Engine warnings (each retry with its reason, a failed progress save) print to
stderr; `-v` also shows informational events such as a fallback to one
connection. A failed download is reported once, as `Error: failed to download
…`. `RUST_LOG` (e.g. `RUST_LOG=debug`) overrides the level and also shows the
engine's own error lines.

| Argument | Description |
|---|---|
| `<url>` | The URL to download (required). |
| `-o, --output <path>` | Output file path. Defaults to a name derived from the URL/headers, saved in the current directory. |
| `--checksum <algo:hex>` | Verify the finished file against a checksum (e.g. `sha256:…`). |
| `-c, --connections <n>` | Max parallel connections, 1–16 (default 4): starts on one and adds more, up to `n`, while measured speed keeps rising. Used only for files ≥ 2 MiB on servers that support range requests; `1` forces a single connection. |
| `--limit-rate <rate>` | Cap the download speed in bytes/s, with an optional `K`/`M`/`G` suffix (×1024), e.g. `500K` or `2M`. Shared across all connections of the download. |
| `-v, --verbose` | Also log informational engine events (download start, fallbacks) to stderr. |

Run `rdm --help` for the full, generated help, and `rdm --version` for the
version.

## Desktop GUI

The GUI lives in [`gui/`](gui/) as a separate Tauri app (the `rdm-gui`
workspace crate) that drives the same engine — so the CLI and engine never
depend on Tauri, Node, or a webview, and a headless server build stays fully
functional. Your download list and history are stored in `~/.rdm/state.json`.

To add a download, click **Add download**, drag a link (or the address bar's
URL) from your browser onto the window, or press Ctrl/Cmd+V with an `http(s)`
URL on the clipboard — the latter two open the Add dialog pre-filled.

Speed limits in the GUI are a number plus a unit picker (KB/s or MB/s, ×1024);
an empty box means no limit, and 0 is rejected. Under **More options**, the Add
dialog takes a checksum and a **Speed limit** for that download.

A limited download shows its cap on its row (`limit 500.0 KB/s`). To set,
change or remove a download's limit while it's downloading, paused or queued,
click that text or the row's speedometer button, then press Enter or click
away. The change applies at once, without restarting the download, and is
kept across restarts. Escape discards the edit, and ✕ removes the limit. A
download's own limit can be higher than the global one, but the lower limit
wins, and the editor says so.

The sliders button in the header opens **Settings**:

- **Display** — theme (System — follows your OS — Light, or Dark) and row
  density (Comfortable or Compact). Remembered per machine in the webview's
  local storage.
- **Downloads** — stored with your download list in `~/.rdm/state.json`:
  - *Max concurrent downloads* (default 3): more downloads than this wait as
    **Queued** ("Waiting for a free slot") and start in the order added as running
    ones finish or are paused. Lowering it never interrupts a running download.
    A queued download can be paused or cancelled.
  - *Global speed limit*: a cap on all downloads combined (empty = no
    limit); applies live, alongside any per-download limit (the lower one
    wins).
  - *Max connections per download* (1–16, default 4): applies the next time a
    download starts.

Values are applied when you press Enter or leave the field; an invalid value
shows an error under it.

Prerequisites: [Node.js](https://nodejs.org/) + [pnpm](https://pnpm.io/), and the
[Tauri system dependencies](https://v2.tauri.app/start/prerequisites/) for your
OS (on Windows, the WebView2 runtime, which ships with Windows 11).

```sh
cd gui
pnpm install
pnpm tauri dev          # run the app in development
pnpm tauri build        # produce a release bundle/installer

pnpm lint               # lint the frontend (oxlint)
pnpm format             # format the frontend (oxfmt); pnpm format:check to verify
pnpm test               # frontend unit tests (vitest)
```

Troubleshooting: the GUI logs to stderr and to a daily file in `~/.rdm`
(`rdm.YYYY-MM-DD.log`, the last 7 kept; dates and rotation are in UTC):
download starts, each retry with its reason, and failures, each line tagged
with its download's `id` and file name. Set `RUST_LOG=debug` for more detail.
An `rdm.log` left by older versions isn't pruned and can be deleted.

Two environment variables relocate the GUI's files, mainly for automated runs:
`RDM_HOME` replaces `~/.rdm` (state, log, and — except on macOS — the webview's
local storage), and `RDM_DOWNLOAD_DIR` replaces the OS download folder as the
Add dialog's default destination.

### GUI end-to-end tests

`gui/e2e/` holds a [WebdriverIO](https://webdriver.io/) suite that drives the
real app through [tauri-driver](https://v2.tauri.app/develop/tests/webdriver/):
adding a download through the Add dialog (and seeing its `Content-Disposition`
name), pause/resume to completion with the bytes checked on disk, a paused
download surviving an app restart, display settings, paste/drop URL intake, and
the download queue (a cap of 1 queues a second download until the first is
paused). It downloads only from the repo's own fixture server
(`cargo run --example fixture_server`) on loopback, and keeps all app state in
a throwaway temp dir via `RDM_HOME`/`RDM_DOWNLOAD_DIR`. It is a separate pnpm
package and not part of `cargo test` or `pnpm test`; CI runs it on Linux.

One-time setup:

- `cargo install tauri-driver --locked`
- Windows: [Microsoft Edge WebDriver](https://developer.microsoft.com/microsoft-edge/tools/webdriver/)
  matching your WebView2 runtime version; put `msedgedriver.exe` on `PATH` or
  point `MSEDGEDRIVER_PATH` at it.
- Linux: `webkit2gtk-driver` (provides `WebKitWebDriver`), plus `xvfb` when
  headless.

```sh
cd gui/e2e
pnpm install
pnpm e2e                # build the app + fixture server, then run the suite
pnpm test               # re-run against the existing build
xvfb-run -a pnpm test   # headless Linux
```

On Windows, build from a shell with the MSVC environment loaded.

## Development

The architecture is an **engine/frontend split**: all download logic lives in
the `rdm` library crate (the `engine` module) with a clean API; both the `rdm`
CLI binary and the GUI are thin frontends over it. The repo is a Cargo
workspace — `rdm` at the root, `rdm-gui` under `gui/src-tauri` — with
`default-members = ["."]` so the commands below build and test only the
engine/CLI (fast, no webview deps). See [`AGENTS.md`](AGENTS.md) for the
conventions and the [roadmap](roadmap.md) for what's planned.

```sh
cargo build                 # build the engine + CLI
cargo test                  # fast, offline test suite (the default gate)
cargo test -- --ignored     # add the live network smoke tests
cargo clippy -- -D warnings # lint (warnings are errors)
cargo fmt --all --check     # formatting check (all workspace crates)
cargo build -p rdm-gui      # build the GUI's Rust crate (needs gui/ deps built)
```

Tests are deterministic and offline by default — they drive the engine against
an in-process HTTP server (including the `Manager` the GUI uses, in
`tests/manager.rs`). Two `#[ignore]`d live smoke tests exercise real network
downloads (single-connection and multi-segment) against pinned public assets.

## License

Dual-licensed under either [MIT](https://opensource.org/licenses/MIT) or
[Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0), at your option.
