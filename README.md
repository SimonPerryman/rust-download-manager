# rdm — Rust Download Manager

Download big files faster and never start over. rdm splits each file across
several connections, pauses and resumes across restarts, and comes as both a
command-line tool and a desktop app.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/main-dark.png">
  <img alt="The rdm desktop app: a sidebar of status and folder filters, and a table of downloads that are done, downloading, waiting, paused and failed" src="docs/screenshots/main-light.png">
</picture>

## Highlights

- **Faster downloads:** each large file is split across several connections.
- **Never start over:** resumes after a crash, restart or Ctrl-C.
- **In control:** pause, resume, stop and speed limits.
- **A queue (GUI):** downloads wait their turn; paste many links at once.
- **Verified:** optional SHA-256 checksum checked before the file is saved.
- **Safe names:** files stay in your folder and never overwrite another.
- **See it work (GUI):** live connection lanes, a speed graph and a per-download log.
- **One engine:** the CLI and the desktop app share it.

[How it works →](docs/how-it-works.md)

## Install

Prebuilt binaries aren't available yet. They're planned for
[M6](milestones/milestone-06-releases-install.md) and will be published on
[GitHub Releases](https://github.com/SimonPerryman/rust-download-manager/releases).
Until then, build from source.

**CLI.** With [Rust](https://www.rust-lang.org/tools/install) installed:

```sh
cargo install --locked --git https://github.com/SimonPerryman/rust-download-manager rdm
```

**Desktop GUI.** Build it from `gui/` with `pnpm tauri build`; see
[CONTRIBUTING.md](CONTRIBUTING.md#building-the-desktop-gui) for the
prerequisites.

## Quick start

```sh
# Download into the current directory (the file name comes from the server):
rdm https://example.com/big-file.zip

# Choose where to save it, and use up to 8 connections:
rdm https://example.com/big-file.zip -o ~/Downloads/archive.zip -c 8

# Cap the speed at 2 MiB/s:
rdm https://example.com/big-file.zip --limit-rate 2M

# Verify the result against a SHA-256 checksum:
rdm https://example.com/big-file.zip --checksum sha256:<hex>
```

Interrupted, or pressed Ctrl-C? Run the same command again to resume. The
[CLI reference](docs/cli.md) has every flag.

## Documentation

- [CLI reference](docs/cli.md): flags, resuming, exit codes and logging.
- [Using the GUI](docs/gui.md): the window, shortcuts, settings and where
  state is kept.
- [How it works](docs/how-it-works.md): segmenting, resume, retries and
  integrity checks.
- [Contributing](CONTRIBUTING.md): building and testing from source.

## Project status

Early, but usable. M0–M5 (the engine, CLI, GUI, parallel downloads, queue and
speed limits, and a GUI redesign) are done; M6, prebuilt releases, comes
next. See the [roadmap](roadmap.md).

## Security

Please report vulnerabilities privately through GitHub's
[Report a vulnerability](https://github.com/SimonPerryman/rust-download-manager/security/advisories/new)
form, not in a public issue.

## Licence

Dual-licensed under either [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option. Bundled third-party icons and
fonts are credited in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
