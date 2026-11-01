# rdm

A download manager in Rust. `rdm` downloads files from a URL — on the command
line or through a desktop GUI — streaming to disk with a live progress readout.

> **Status: early.** M0 + M1 are a robust single-connection CLI download
> (pause/resume, resume-after-restart, retry, integrity checking); M2 adds a
> desktop GUI over the same engine. Parallel segments and a download queue are
> on the [roadmap](roadmap.md), not built yet.

## Features

- Download over HTTP/HTTPS, streamed to disk chunk-by-chunk (no full buffering).
- Follows redirects and handles common HTTP status codes.
- Derives the output filename from `Content-Disposition` or the URL, or takes
  an explicit path.
- Live progress bar with size, speed, and ETA.
- Resumable: data streams to a `<file>.part` with a `<file>.rdm` sidecar;
  re-running the same command continues where an interrupted download left off
  (via HTTP range requests, validated against `ETag`/`Last-Modified`).
- Retries transient failures (timeouts, dropped connections, 5xx/408/429) with
  exponential backoff.
- Verifies the finished file's size, and an optional `--checksum sha256:<hex>`,
  before moving it into place; never clobbers an existing file (`name (1).ext`).
- Optional **desktop GUI** (Tauri + React): manage many downloads at once —
  add/pause/resume/cancel, live progress/speed/ETA, and an Active/History list
  that persists across restarts. The GUI is an *additive* frontend; the CLI and
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

# Interrupted? Re-run the same command to resume from where it stopped.
```

| Argument | Description |
|---|---|
| `<url>` | The URL to download (required). |
| `-o, --output <path>` | Output file path. Defaults to a name derived from the URL/headers, saved in the current directory. |
| `--checksum <algo:hex>` | Verify the finished file against a checksum (e.g. `sha256:…`). |

Run `rdm --help` for the full, generated help, and `rdm --version` for the
version.

## Desktop GUI

The GUI lives in [`gui/`](gui/) as a separate Tauri app (the `rdm-gui`
workspace crate) that drives the same engine — so the CLI and engine never
depend on Tauri, Node, or a webview, and a headless server build stays fully
functional. Your download list and history are stored in `~/.rdm/state.json`.

Prerequisites: [Node.js](https://nodejs.org/) + npm, and the
[Tauri system dependencies](https://v2.tauri.app/start/prerequisites/) for your
OS (on Windows, the WebView2 runtime, which ships with Windows 11).

```sh
cd gui
npm install
npm run tauri dev       # run the app in development
npm run tauri build     # produce a release bundle/installer
```

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
cargo test -- --ignored     # add the live network smoke test
cargo clippy -- -D warnings # lint (warnings are errors)
cargo fmt --all --check     # formatting check (all workspace crates)
cargo build -p rdm-gui      # build the GUI's Rust crate (needs gui/ deps built)
```

Tests are deterministic and offline by default — they drive the engine against
an in-process HTTP server (including the `Manager` the GUI uses, in
`tests/manager.rs`). A single `#[ignore]`d live smoke test exercises a real
network download against a pinned public asset.

## License

Dual-licensed under either [MIT](https://opensource.org/licenses/MIT) or
[Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0), at your option.
