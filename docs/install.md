# Installing rdm

Every release on the
[Releases page](https://github.com/SimonPerryman/rust-download-manager/releases/latest)
has the command-line tool (CLI) for Windows, macOS and Linux, and installers
for the desktop app. Neither needs Rust or Node.js.

## Contents

- [Command-line tool](#command-line-tool)
- [Desktop app](#desktop-app)
- [Unsigned builds: Windows and macOS warnings](#unsigned-builds-windows-and-macos-warnings)
- [Updating](#updating)
- [Building from source](#building-from-source)

## Command-line tool

**macOS and Linux**, with the shell installer:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/SimonPerryman/rust-download-manager/releases/latest/download/rdm-installer.sh | sh
```

**Windows**, with the PowerShell installer:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/SimonPerryman/rust-download-manager/releases/latest/download/rdm-installer.ps1 | iex"
```

**Homebrew** (macOS and Linux):

```sh
brew install simonperryman/tap/rdm
```

The shell and PowerShell installers put `rdm` in `~/.cargo/bin` (on
Windows, `%USERPROFILE%\.cargo\bin`) and add that folder to your `PATH`; open
a new terminal afterwards. Check it with `rdm --version`.

To install by hand instead, download the archive for your platform from the
Releases page, check it against its `.sha256` file, and put `rdm` (or
`rdm.exe`) somewhere on your `PATH`:

| Platform | Archive |
|---|---|
| Windows x64 | `rdm-x86_64-pc-windows-msvc.zip` |
| macOS, Apple silicon | `rdm-aarch64-apple-darwin.tar.xz` |
| macOS, Intel | `rdm-x86_64-apple-darwin.tar.xz` |
| Linux x64 | `rdm-x86_64-unknown-linux-gnu.tar.xz` (glibc 2.39 or later) |
| Linux x64, any distribution | `rdm-x86_64-unknown-linux-musl.tar.xz` (static, no glibc needed) |

## Desktop app

Download the installer for your platform from the
[Releases page](https://github.com/SimonPerryman/rust-download-manager/releases/latest)
(`<version>` is the release's version, e.g. `0.1.0`):

| Platform | File |
|---|---|
| Windows x64 | `rdm_<version>_x64-setup.exe` (recommended) or `rdm_<version>_x64_en-US.msi` |
| macOS, Apple silicon | `rdm_<version>_aarch64.dmg` |
| macOS, Intel | `rdm_<version>_x64.dmg` |
| Linux x64 | `rdm_<version>_amd64.AppImage`, or `rdm_<version>_amd64.deb` for Debian and Ubuntu |

On Windows the app uses the WebView2 runtime, which ships with Windows 11
(the installer fetches it on older systems). On Linux it needs glibc 2.35 or
later (Ubuntu 22.04, Debian 12 or newer); the `.deb` also installs the
WebKitGTK 4.1 it depends on. To run the AppImage, make it executable
(`chmod +x rdm_*.AppImage`) and start it.

## Unsigned builds: Windows and macOS warnings

rdm's releases aren't code-signed yet, so the first launch shows a warning.

- **Windows:** SmartScreen says "Windows protected your PC". Choose
  **More info**, then **Run anyway**.
- **macOS:** the first launch of the app from the `.dmg` is blocked ("can't be
  opened because Apple cannot check it"). Open **System Settings**, then
  **Privacy & Security**, and choose **Open Anyway**. Or, in a terminal:
  `xattr -dr com.apple.quarantine /Applications/rdm.app`.

The CLI installed by the shell installer or Homebrew isn't affected.

## Updating

- **CLI, shell or PowerShell installer:** run the same one-liner again; it
  replaces `rdm` in place.
- **CLI, Homebrew:** `brew upgrade rdm`.
- **Desktop app:** it updates itself: when a newer version is out it offers
  **Restart now** or **Install when I quit** (see
  [Updates](gui.md#updates)). Installed from a `.deb`, download the new `.deb`
  from the release page it links to and install it over the old one. Your
  download list and settings are kept: they live in `~/.rdm`, not in the app.
- **Desktop app 0.2.0 or earlier:** these have no updater, so update once by
  hand: download and install the new version from the Releases page over the
  old one. From then on it updates itself.

## Building from source

With [Rust](https://www.rust-lang.org/tools/install) installed:

```sh
cargo install --locked --git https://github.com/SimonPerryman/rust-download-manager rdm
```

To build the desktop app, see
[CONTRIBUTING.md](../CONTRIBUTING.md#building-the-desktop-gui).
