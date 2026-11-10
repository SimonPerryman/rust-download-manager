// WebdriverIO config for the GUI's webview E2E suite. Drives the real built
// app (`pnpm run build` first) through tauri-driver, against the Rust fixture
// server on loopback — never the real network — with all app state in a
// throwaway temp dir.

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { baseConfig, startFixture, startTauriDriver, stopAll } from "./launch";
import { bundleKind, startUpdateFeed } from "./updateFeed";

// One temp root per run: created when the launcher loads this config, and
// inherited (via the environment) by the workers that load it again.
process.env.RDM_E2E_ROOT ??= mkdtempSync(path.join(os.tmpdir(), "rdm-e2e-"));
const e2eRoot = process.env.RDM_E2E_ROOT;
const rdmHome = path.join(e2eRoot, "home");

// Slow enough that a click lands mid-stream (~100 KB in 4 KB chunks ≈ 5 s).
const SLOW_DELAY_MS = 200;
// Slows each /seg connection (4 MiB in 32 KiB chunks ≈ 13 s on one) so a
// download outlasts the engine's ramp-up windows and visibly adds connections.
const SEG_DELAY_MS = 100;

export const config: WebdriverIO.Config = {
  ...baseConfig,
  specs: ["./specs/**/*.e2e.ts"],
  mochaOpts: { ui: "bdd", timeout: 90_000 },

  // Runs once in the launcher; workers (and through tauri-driver, the app)
  // inherit the environment set here.
  onPrepare: async () => {
    const dirs = {
      RDM_HOME: rdmHome,
      RDM_DOWNLOAD_DIR: path.join(e2eRoot, "downloads"),
    };
    for (const [key, dir] of Object.entries(dirs)) {
      mkdirSync(dir, { recursive: true });
      process.env[key] = dir;
    }
    // open_download / reveal_download log here instead of launching an OS handler.
    process.env.RDM_OPEN_LOG = path.join(e2eRoot, "opened.log");
    console.log(`E2E temp root: ${e2eRoot}`);

    // Automatic update checks answered "no", so only the updater spec ever
    // sees the first-run question or checks (against the local feed).
    writeFileSync(path.join(rdmHome, "updater.json"), JSON.stringify({ auto_check: false }));
    const feed = await startUpdateFeed(e2eRoot);
    process.env.RDM_E2E_UPDATE_FEED = feed.base;
    process.env.RDM_UPDATE_ENDPOINT = `${feed.base}/latest.json`;
    process.env.RDM_UPDATE_PUBKEY = feed.pubkey;
    process.env.RDM_UPDATE_BUNDLE = bundleKind();
    process.env.RDM_UPDATE_DRY_INSTALL = "1";

    process.env.RDM_E2E_BASE_URL = await startFixture([
      "--slow-delay-ms",
      String(SLOW_DELAY_MS),
      "--seg-delay-ms",
      String(SEG_DELAY_MS),
    ]);
    await startTauriDriver();
  },

  // A failed run keeps its temp root (state.json, rdm.*.log, opened.log, downloads) to inspect.
  onComplete: (exitCode: number) => {
    stopAll();
    if (exitCode === 0) rmSync(e2eRoot, { recursive: true, force: true });
  },
};
