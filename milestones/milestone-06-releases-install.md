---
id: M6
title: Releases & install
status: todo
depends-on: [M5]
tags: [cli, gui, ci]
---

## Goal
A newcomer can install rdm, the CLI or the desktop GUI, from a GitHub
Release without a Rust toolchain.

## Context / rationale
Today the only install path is `cargo install --path .` from a clone, and the
GUI has no install path at all (`pnpm tauri build` locally). That limits rdm
to people who already have Rust, Node and pnpm set up.

- **crates.io isn't an option.** The crate name `rdm` is taken there, and both
  crates set `publish = false`. Distribution is through GitHub Releases, plus
  installers and a Homebrew tap that point at them.
- **CLI: `dist`** (formerly cargo-dist). It builds per-target archives,
  generates shell and PowerShell installers and a Homebrew formula, and writes
  its own release workflow.
- **GUI: the official `tauri-apps/tauri-action`.** It builds the platform
  bundles and uploads them to a GitHub Release.
- **Both attach to the same release**, created by one tag push.

The CI pinning rules in [AGENTS.md](../AGENTS.md) (Toolchain) apply to the
release workflow like any other, and `gui/scripts/guard-ci-pins.mjs` enforces
them. Roadmap: [roadmap.md](../roadmap.md).

## Scope
**In:**
- **CLI release assets (dist)**, one archive per target with a SHA-256
  checksum file:
  - Windows x64: `x86_64-pc-windows-msvc`;
  - macOS arm64 and x64: `aarch64-apple-darwin`, `x86_64-apple-darwin`;
  - Linux x64: `x86_64-unknown-linux-gnu` and `x86_64-unknown-linux-musl`.
    The musl build is static, so it runs on distributions whose glibc is
    older than the build image's. reqwest uses rustls, so there's no OpenSSL
    to link.
- **Installers:**
  - shell: `curl --proto '=https' --tlsv1.2 -LsSf https://github.com/SimonPerryman/rust-download-manager/releases/latest/download/rdm-installer.sh | sh`;
  - PowerShell: `powershell -ExecutionPolicy Bypass -c "irm https://github.com/SimonPerryman/rust-download-manager/releases/latest/download/rdm-installer.ps1 | iex"`;
  - Homebrew: `brew install simonperryman/tap/rdm`, a formula dist publishes
    to the `SimonPerryman/homebrew-tap` repo (owner setup, below).
  - Optional: a CLI `.msi` from dist (needs `[package.metadata.wix]` with a
    fixed upgrade GUID). Skip it if the GUI's installer is enough for Windows
    users.
- **GUI release assets (tauri-action)**, attached to the same release:
  - Windows: NSIS `.exe` setup, and `.msi`;
  - macOS: `.dmg` for arm64 and x64;
  - Linux: AppImage and `.deb`.
- **Tauri identifier** changes from `dev.rdm.gui` to
  `io.github.simonperryman.rdm` before the first release. The identifier
  names the app on each OS (bundle id, install keys, webview data folder),
  and changing it after a release strands installed copies.
  - Effect: the webview's app-data folder moves, so the GUI's local prefs
    (theme, density, remembered folder, "opened once") start fresh. The
    download list in `~/.rdm/state.json` doesn't depend on the identifier
    and is unaffected.
  - There are no users yet, so no migration is needed.
- **App metadata:**
  - `tauri.conf.json`: `bundle.publisher`, `bundle.copyright`,
    `bundle.license` (`MIT OR Apache-2.0`), `bundle.homepage`,
    `bundle.category` (`Utility`), short and long descriptions.
  - `gui/src-tauri/Cargo.toml`: `repository`, matching the root crate.
- **One version for the CLI and GUI** (see Versioning, below).
- **Release notes from `CHANGELOG.md`.**
- **README install section** pointing at real downloads, with an honest note
  on unsigned builds.

**Out (deferred):**
- **Code signing and notarisation** (backlog). The first release is
  unsigned:
  - Windows SmartScreen shows "Windows protected your PC" for an unknown
    publisher. Users choose More info, then Run anyway.
  - macOS Gatekeeper blocks the first launch of the `.dmg` app ("can't be
    opened because Apple cannot check it"). Users open System Settings,
    Privacy & Security, and choose Open Anyway, or run
    `xattr -dr com.apple.quarantine /Applications/rdm.app`. The CLI is
    unaffected when installed by the shell installer or Homebrew, since
    neither sets the quarantine flag.
  - Follow-up options: **Azure Trusted Signing** for Windows (about
    US$10/month for the basic tier; check eligibility for individual
    developers in your region) and an **Apple Developer Program**
    membership (about US$99/year) for signing and notarising the macOS
    app.
- **In-app updates** (Tauri updater plugin). It needs signed update
  manifests and a signing key, so it follows signing (backlog).
- **Package-manager manifests** beyond Homebrew: winget, scoop (stretch,
  below).
- **crates.io** publishing: the name is taken. Revisit only if the crate is
  renamed.

## Versioning
- **One version, one tag.** The CLI crate, the GUI crate, `tauri.conf.json`
  and `gui/package.json` all carry the same version, set once in
  `[workspace.package] version` and inherited with `version.workspace =
  true`. `tauri.conf.json` drops its `version` field so Tauri reads the
  GUI crate's. A tag `vX.Y.Z` (or `vX.Y.Z-rc.N`) on `main` releases
  exactly that version.
  - Why: users get "rdm 0.2.0" for both; a split version would need two
    tags, two releases and a compatibility table for one shared engine.
- **Release notes: `CHANGELOG.md`** (Keep a Changelog format), not GitHub's
  generated notes.
  - Why: dist reads the matching `## X.Y.Z` section into the release body,
    so the notes are reviewed in the version-bump PR, written for users
    rather than as a list of PR titles, and kept in the repo.
  - The bump PR moves `## Unreleased` to `## X.Y.Z - <date>`.

## Tasks
### Version, identifier and metadata
- [ ] Move the version to `[workspace.package]`; both crates inherit it;
  remove `version` from `tauri.conf.json`; keep `gui/package.json` in step.
- [ ] Add `scripts/check-release-version` (or a Rust test) that fails unless
  the tag, the workspace version, `gui/package.json` and the `CHANGELOG.md`
  heading agree. The release workflow runs it first.
- [ ] Change the identifier to `io.github.simonperryman.rdm`.
- [ ] Fill `bundle.publisher`, `copyright`, `license`, `homepage`,
  `category` and descriptions in `tauri.conf.json`; add `repository` to
  `gui/src-tauri/Cargo.toml`.
- [ ] Add `CHANGELOG.md` with an `## Unreleased` section covering M0 to M5.

### CLI release (dist)
- [ ] `dist init` with the targets, installers (`shell`, `powershell`,
  `homebrew`, optionally `msi`) and `tap = "SimonPerryman/homebrew-tap"`,
  `publish-jobs = ["homebrew"]`. Pin `cargo-dist-version`.
- [ ] Adjust the generated `.github/workflows/release.yml` to the repo's
  rules, then set `allow-dirty = ["ci"]` so `dist` doesn't regenerate it:
  - every `uses:` pinned to a full commit SHA with a `# vX.Y.Z` comment;
  - exact runner images: `ubuntu-24.04`, `windows-2025-vs2026`, `macos-15`
    (arm64, also building `x86_64-apple-darwin`), and `macos-15-intel` where
    an x64 Mac must run a binary;
  - dist itself installed at the pinned version (its installer URL names
    the version; any `cargo install` passes `--version`);
  - the generated `pull_request` trigger removed: the workflow runs on tag
    push only;
  - top-level `permissions: contents: read`; `contents: write` only on the
    job that creates the release and uploads assets.
- [ ] Owner setup steps (below) done before the first non-rc tag.

### GUI release (tauri-action)
- [ ] A GUI job (or reusable workflow called from `release.yml`) runs
  `tauri-apps/tauri-action` pinned by SHA, in a matrix of
  `windows-2025-vs2026`, `macos-15` (`--target aarch64-apple-darwin` and
  `--target x86_64-apple-darwin`) and `ubuntu-24.04` (with the same apt
  packages as the e2e job). Node and pnpm are pinned as in `ci.yml`.
- [ ] It uploads to the release dist creates for the tag, not a second
  release. Wire it as a dist custom job if that orders cleanly (dist's
  `host` job creates the release first); otherwise have one job create a
  draft release, both upload to it, and a final job publishes it. Decide in
  the PR and note why.
- [ ] `contents: write` is scoped to the upload job only.

### Smoke and checks
- [ ] A `smoke` job in the release workflow, after the build jobs, on each
  CLI target's native runner (`ubuntu-24.04` for gnu and musl,
  `windows-2025-vs2026`, `macos-15`, `macos-15-intel`):
  - download the target's archive from the release and check it against
    its SHA-256 file;
  - run `rdm --version` and compare it to the tag;
  - serve a generated file over local HTTP (`python3 -m http.server`) and
    run `rdm http://127.0.0.1:<port>/<file> --checksum sha256:<hex>`;
    expect exit code 0.
- [ ] A guard, `gui/scripts/guard-release-workflow.mjs`, part of `pnpm lint`
  next to `guard-ci-pins.mjs`. It fails if `release.yml`:
  - has any trigger other than `push: tags`;
  - grants `contents: write` (or any `write`) at the top level, or to a job
    other than the release/upload jobs.
- [ ] Regular CI runs `dist plan` (pinned) so a broken dist config fails a
  PR, not a tag.

### Docs
- [ ] README **Installation**: the shell, PowerShell and Homebrew one-liners;
  GUI downloads by platform from the Releases page; the unsigned-build notes
  for SmartScreen and Gatekeeper; building from source as the last option.
- [ ] README: how to cut a release (bump PR, then tag), in **Development**.

### Stretch
- [ ] winget manifest (`winget-pkgs` PR) and/or a scoop bucket.
- [ ] `cargo binstall` metadata in `[package.metadata.binstall]`. binstall
  normally resolves a crate through crates.io; check that `cargo binstall
  --git https://github.com/SimonPerryman/rust-download-manager rdm` finds
  the release assets, and drop it if not.

## Owner setup (before the first non-rc release)
1. Create the public repo `SimonPerryman/homebrew-tap` (Homebrew's naming:
   `brew install simonperryman/tap/rdm` reads `homebrew-tap`).
2. Create a fine-grained token with **Contents: read and write** on
   `homebrew-tap` only, and add it to this repo's Actions secrets as
   `HOMEBREW_TAP_TOKEN`. dist's `publish-homebrew-formula` job uses it.
3. Optional: a `release` environment with required reviewers, so a tag push
   waits for approval before the write-scoped jobs run.

## Acceptance criteria
_Standing gates (fmt/clippy/test/CI) live in AGENTS.md's Definition of Done
and apply here without being restated._

- [ ] Pushing tag `v0.1.0-rc.1` produces one GitHub (pre-)release holding:
  - CLI archives plus `.sha256` files for all five targets;
  - `rdm-installer.sh` and `rdm-installer.ps1`;
  - the generated Homebrew formula `rdm.rb`;
  - GUI bundles: NSIS `.exe` and `.msi`, `.dmg` for arm64 and x64,
    `.AppImage` and `.deb`.
  Checked by a final workflow step that lists the release's assets with
  `gh release view <tag> --json assets` and fails on any missing pattern.
- [ ] The `smoke` job passes on every CLI target: checksum file matches,
  `rdm --version` prints the tag's version, and the local-HTTP download with
  `--checksum` exits 0.
- [ ] The version check fails the workflow, before any build, when the tag,
  workspace version, `gui/package.json` and `CHANGELOG.md` heading disagree.
  (Run it locally with a mismatched tag to show it fails.)
- [ ] `pnpm lint` passes, including `guard-ci-pins.mjs` over `release.yml`
  and the new `guard-release-workflow.mjs`.
- [ ] `guard-release-workflow.mjs` fails on a copy of `release.yml` with a
  `pull_request:` trigger added, and on one with top-level
  `contents: write`. (A guard self-test, run in `pnpm lint` or vitest.)
- [ ] The release workflow doesn't run on pull requests: the PR that adds it
  shows no `Release` run in its checks.
- [ ] `tauri.conf.json` has identifier `io.github.simonperryman.rdm`, and
  `bundle.publisher`, `copyright` and `license` are set (asserted by the
  guard or a Rust test reading the file); the existing e2e suite passes
  under the new identifier.
- [ ] After tagging `v0.1.0`, a `verify-install` job on `macos-15` runs
  `brew install simonperryman/tap/rdm && rdm --version`, and on
  `ubuntu-24.04` and `windows-2025-vs2026` runs the shell and PowerShell
  one-liners followed by `rdm --version`.
- [ ] README's Installation section shows the one-liners and the unsigned
  build notes, and its links resolve against the `v0.1.0` release.

## Open questions
1. **Linux GUI glibc floor:** the AppImage and `.deb` built on
   `ubuntu-24.04` need glibc 2.39. Build the Linux GUI on `ubuntu-22.04`
   (still an exact pin) for wider reach, or accept 24.04 as the floor?
2. **CLI MSI:** ship it, or let the GUI's installers cover Windows?
3. **GUI and dist ordering:** a dist custom job, or a draft release that
   both upload to (Tasks, GUI release)?
