// WebdriverIO config for the README screenshots (`pnpm screenshots`), kept
// apart from the e2e suite (whose specs glob doesn't match it). Drives the
// real built app against the fixture server and saves PNGs to
// docs/screenshots/. App state and downloads live under a short, neutral dir
// (target/screenshots by default) because folder names show in the sidebar.

import { mkdirSync, rmSync } from "node:fs";
import path from "node:path";
import { baseConfig, repoRoot, startFixture, startTauriDriver, stopAll } from "./launch";

const shotsRoot = process.env.RDM_SHOTS_ROOT ?? path.join(repoRoot, "target", "screenshots");

// The largest file the session serves; the fixture holds this many bytes in memory.
const SEG_LEN = 100 * 1000 * 1000;
// Per 32 KiB chunk and connection: about 0.5 MB/s on one connection, so
// downloads are visibly mid-way when the shots are taken and the speed graph
// fills for half a minute before the details shot.
const SEG_DELAY_MS = 60;

export const config: WebdriverIO.Config = {
  ...baseConfig,
  specs: ["./screenshots/*.shots.ts"],
  mochaOpts: { ui: "bdd", timeout: 300_000 },

  onPrepare: async () => {
    rmSync(shotsRoot, { recursive: true, force: true });
    const dirs = {
      RDM_HOME: path.join(shotsRoot, "home"),
      RDM_DOWNLOAD_DIR: path.join(shotsRoot, "Downloads"),
    };
    for (const [key, dir] of Object.entries(dirs)) {
      mkdirSync(dir, { recursive: true });
      process.env[key] = dir;
    }
    // Open logs here instead of launching an OS handler.
    process.env.RDM_OPEN_LOG = path.join(shotsRoot, "opened.log");
    process.env.RDM_SHOTS_OUT = path.join(repoRoot, "docs", "screenshots");

    process.env.RDM_E2E_BASE_URL = await startFixture([
      "--seg-len",
      String(SEG_LEN),
      "--seg-delay-ms",
      String(SEG_DELAY_MS),
    ]);
    await startTauriDriver();
  },

  onComplete: () => stopAll(),
};
