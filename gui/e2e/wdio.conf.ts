// WebdriverIO config for the GUI's webview E2E suite. Drives the real built
// app (`pnpm run build` first) through tauri-driver, against the Rust fixture
// server on loopback — never the real network — with all app state in a
// throwaway temp dir.

import { spawn, type ChildProcess } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { createInterface } from "node:readline";

const isWindows = process.platform === "win32";
const exe = (p: string) => (isWindows ? `${p}.exe` : p);

const repoRoot = path.resolve(import.meta.dirname, "../..");
const targetDir = process.env.CARGO_TARGET_DIR ?? path.join(repoRoot, "target");
const appBinary = exe(path.join(targetDir, "debug", "rdm-gui"));
const fixtureBinary = exe(path.join(targetDir, "debug", "examples", "fixture_server"));
const tauriDriverBinary =
  process.env.TAURI_DRIVER_PATH ?? exe(path.join(os.homedir(), ".cargo", "bin", "tauri-driver"));

// One temp root per run: created when the launcher loads this config, and
// inherited (via the environment) by the workers that load it again.
process.env.RDM_E2E_ROOT ??= mkdtempSync(path.join(os.tmpdir(), "rdm-e2e-"));
const e2eRoot = process.env.RDM_E2E_ROOT;
const rdmHome = path.join(e2eRoot, "home");

const DRIVER_PORT = 4444;
// Slow enough that a click lands mid-stream (~100 KB in 4 KB chunks ≈ 5 s).
const SLOW_DELAY_MS = 200;
// Slows each /seg connection (4 MiB in 32 KiB chunks ≈ 13 s on one) so a
// download outlasts the engine's ramp-up windows and visibly adds connections.
const SEG_DELAY_MS = 100;

let fixture: ChildProcess | undefined;
let tauriDriver: ChildProcess | undefined;

/** Start the fixture server on a free port and resolve its base URL. */
function startFixture(): Promise<string> {
  return new Promise((resolve, reject) => {
    const args = [
      "--port",
      "0",
      "--slow-delay-ms",
      String(SLOW_DELAY_MS),
      "--seg-delay-ms",
      String(SEG_DELAY_MS),
    ];
    const child = spawn(fixtureBinary, args, { stdio: ["ignore", "pipe", "inherit"] });
    fixture = child;
    child.once("error", reject);
    child.once("exit", (code) => reject(new Error(`fixture server exited (${code})`)));
    createInterface({ input: child.stdout! }).on("line", (line) => {
      const match = /^listening on (\S+)/.exec(line);
      if (match) resolve(match[1]);
    });
  });
}

/** Resolve once something accepts TCP connections on `port`. */
async function waitForPort(port: number, timeoutMs = 15_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const open = await new Promise<boolean>((resolve) => {
      const socket = net.connect(port, "127.0.0.1");
      socket.once("connect", () => {
        socket.destroy();
        resolve(true);
      });
      socket.once("error", () => resolve(false));
    });
    if (open) return;
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`nothing listening on port ${port} after ${timeoutMs} ms`);
}

export const config: WebdriverIO.Config = {
  runner: "local",
  hostname: "127.0.0.1",
  port: DRIVER_PORT,
  specs: ["./specs/**/*.e2e.ts"],
  maxInstances: 1,
  capabilities: [
    {
      // tauri-driver translates this into the native driver's capabilities.
      "tauri:options": { application: appBinary },
    } as WebdriverIO.Capabilities,
  ],
  logLevel: "warn",
  waitforTimeout: 15_000,
  connectionRetryTimeout: 60_000,
  connectionRetryCount: 2,
  framework: "mocha",
  reporters: ["spec"],
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
    // open_path / reveal_path log here instead of launching an OS handler.
    process.env.RDM_OPEN_LOG = path.join(e2eRoot, "opened.log");
    console.log(`E2E temp root: ${e2eRoot}`);

    process.env.RDM_E2E_BASE_URL = await startFixture();

    const driverArgs = process.env.MSEDGEDRIVER_PATH
      ? ["--native-driver", process.env.MSEDGEDRIVER_PATH]
      : [];
    tauriDriver = spawn(tauriDriverBinary, driverArgs, {
      stdio: ["ignore", "inherit", "inherit"],
    });
    tauriDriver.once("error", (e) => console.error(`tauri-driver failed to start: ${e}`));
    await waitForPort(DRIVER_PORT);
  },

  // A failed run keeps its temp root (state.json, rdm.*.log, opened.log, downloads) to inspect.
  onComplete: (exitCode: number) => {
    tauriDriver?.kill();
    fixture?.kill();
    if (exitCode === 0) rmSync(e2eRoot, { recursive: true, force: true });
  },
};
