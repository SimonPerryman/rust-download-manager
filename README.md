# rdm

A download manager in Rust. `rdm` downloads a file from a URL on the command
line, streaming it to disk with a live progress readout.

> **Status: early.** This is the M0 "walking skeleton" — a single-connection
> CLI download that works end to end. Pause/resume, retry, and parallel
> segments are on the [roadmap](roadmap.md), not built yet.

## Features

- Download over HTTP/HTTPS, streamed to disk chunk-by-chunk (no full buffering).
- Follows redirects and handles common HTTP status codes.
- Derives the output filename from `Content-Disposition` or the URL, or takes
  an explicit path.
- Live progress bar with size, speed, and ETA.

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
```

| Argument | Description |
|---|---|
| `<url>` | The URL to download (required). |
| `-o, --output <path>` | Output file path. Defaults to a name derived from the URL/headers, saved in the current directory. |

Run `rdm --help` for the full, generated help, and `rdm --version` for the
version.

## Development

The architecture is an **engine/frontend split**: all download logic lives in
the `rdm` library crate (the `engine` module) with a clean API; the `rdm`
binary is a thin CLI frontend over it. See [`AGENTS.md`](AGENTS.md) for the
conventions and the [roadmap](roadmap.md) for what's planned.

```sh
cargo build                 # build
cargo test                  # fast, offline test suite (the default gate)
cargo test -- --ignored     # add the live network smoke test
cargo clippy -- -D warnings # lint (warnings are errors)
cargo fmt --check           # formatting check
```

Tests are deterministic and offline by default — they drive the engine against
an in-process HTTP server. A single `#[ignore]`d live smoke test exercises a
real network download against a pinned public asset.

## License

Dual-licensed under either [MIT](https://opensource.org/licenses/MIT) or
[Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0), at your option.
