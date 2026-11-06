---
name: rdm-e2e
description: Run rdm's webview E2E suite (tauri-driver + WebdriverIO in gui/e2e) against the real GUI and a local fixture server, and report pass/fail. The e2e provider for /factory:e2e-verify; also use for "run the GUI e2e tests".
---

# rdm-e2e — run the GUI webview E2E suite

The suite in `gui/e2e` is self-contained: `pnpm e2e` builds the fixture server
(`examples/fixture_server.rs`) and the app (`tauri build --debug --no-bundle`),
then WebdriverIO starts the fixture + `tauri-driver`, drives the real app, and
tears everything down. No dev stack is needed. Runs are isolated via
`RDM_HOME` / `RDM_DOWNLOAD_DIR` (set by `wdio.conf.ts`), so the developer's
`~/.rdm` and Downloads folder are never touched. Specs: `gui/e2e/specs/*.e2e.ts`.

## 1. Preflight — stop and report exactly what's missing
- `tauri-driver` on PATH (`cargo install tauri-driver --locked`), or `TAURI_DRIVER_PATH` set.
- `gui/e2e/node_modules` present — else `pnpm install --frozen-lockfile` in `gui/e2e`
  (and in `gui/` if its `node_modules` is missing). pnpm only, never npm.
- **Windows:** an `msedgedriver.exe` matching the installed WebView2 version.
  Default location `%LOCALAPPDATA%\rdm-e2e-tools\edgedriver\msedgedriver.exe`; export
  it as `MSEDGEDRIVER_PATH`. If missing or mismatched, say so and point to
  https://developer.microsoft.com/microsoft-edge/tools/webdriver/ — don't guess.
- **Linux:** `webkit2gtk-driver` and `xvfb` installed (see the `e2e` job in
  `.github/workflows/ci.yml`).

## 2. Run
- **Windows** — from PowerShell, NOT the Bash tool (Git Bash's `link` shadows
  MSVC `link.exe`). Load vcvars, then run in the background (build is slow):
  ```
  $env:MSEDGEDRIVER_PATH = "$env:LOCALAPPDATA\rdm-e2e-tools\edgedriver\msedgedriver.exe"
  cmd /c '"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1 && cd gui\e2e && pnpm e2e 2>&1'
  ```
  (If that vcvars path doesn't exist, find `vcvars64.bat` under
  `C:\Program Files (x86)\Microsoft Visual Studio\2022\`.)
- **Linux:** `cd gui/e2e && pnpm run build && xvfb-run -a pnpm test`
- **macOS:** unsupported — tauri-driver has no macOS WKWebView driver. Report skip.

## 3. Report
Quote the spec reporter's summary (`N passing` / failures with their assertion
messages). On failure the per-run temp root (app state, downloads, `rdm.*.log`) is
kept — its path is printed by `wdio.conf.ts`; read the `rdm.*.log` there for the
engine-side cause before diagnosing. Never claim a pass you didn't see.

Note: the run overwrites `target/debug/rdm-gui(.exe)` with an embedded-frontend
build; the next `pnpm tauri dev` simply rebuilds it.

## Persisting new coverage
New regression specs go in `gui/e2e/specs/*.e2e.ts` (the `.e2e.ts` suffix keeps
vitest from collecting them). Prefer aria roles/labels and the row's
`data-testid="download-row"` / `data-filename` / `data-status` as selectors.
