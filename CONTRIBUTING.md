# Contributing

How to build and test rdm from source. Conventions for contributors and coding
agents live in [AGENTS.md](AGENTS.md).

## Layout

The repository is a Cargo workspace: the `rdm` engine library and CLI at the
root, and the GUI's Rust crate (`rdm-gui`) under `gui/src-tauri`. All download
logic lives in the engine; the CLI and the GUI are thin frontends over it.

## Building the CLI

You need [Rust](https://www.rust-lang.org/tools/install) (stable; the MSVC
toolchain on Windows, which needs the Visual Studio C++ Build Tools).

`cargo install --locked --git https://github.com/SimonPerryman/rust-download-manager rdm`
builds the `rdm` binary and puts it in `~/.cargo/bin`. The trailing `rdm`
picks the CLI package out of the repository's workspace. From a clone,
`cargo install --locked --path .` does the same.

## Building the desktop GUI

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

To start the GUI, run the app that `pnpm tauri build` produced, or
`pnpm tauri dev` from `gui/` during development.

## Testing

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

The default tests run offline against a local HTTP server. CI builds and tests
on Linux and Windows; macOS isn't tested yet.

The GUI also has an end-to-end suite that drives the real app with
WebdriverIO; see the [`gui/e2e` package](gui/e2e/package.json) (`pnpm e2e`),
which needs `tauri-driver` and, on Windows, Microsoft Edge WebDriver.
`pnpm screenshots` in the same package regenerates the images in
[`docs/screenshots`](docs/screenshots/) used by the README and docs.
Each run picks free ports for `tauri-driver`, so parallel runs (e.g. in
separate worktrees) are safe; set `RDM_E2E_DRIVER_PORT` /
`RDM_E2E_NATIVE_PORT` to pin them.
