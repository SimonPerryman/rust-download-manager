// Starts what a WebdriverIO run drives the app through: the Rust fixture
// server on loopback and tauri-driver. Shared by the e2e suite
// (wdio.conf.ts) and the README screenshot run (wdio.screenshots.conf.ts).

import { spawn, type ChildProcess } from "node:child_process";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { createInterface } from "node:readline";

const isWindows = process.platform === "win32";
const exe = (p: string) => (isWindows ? `${p}.exe` : p);

export const repoRoot = path.resolve(import.meta.dirname, "../..");
const targetDir = process.env.CARGO_TARGET_DIR ?? path.join(repoRoot, "target");
export const appBinary = exe(path.join(targetDir, "debug", "rdm-gui"));
const fixtureBinary = exe(path.join(targetDir, "debug", "examples", "fixture_server"));
const tauriDriverBinary =
  process.env.TAURI_DRIVER_PATH ?? exe(path.join(os.homedir(), ".cargo", "bin", "tauri-driver"));

/** Resolve `count` distinct ports that are free right now on loopback. */
async function freePorts(count: number): Promise<number[]> {
  const servers = await Promise.all(
    Array.from(
      { length: count },
      () =>
        new Promise<net.Server>((resolve, reject) => {
          const server = net.createServer();
          server.once("error", reject);
          server.listen(0, "127.0.0.1", () => resolve(server));
        }),
    ),
  );
  const ports = servers.map((s) => (s.address() as net.AddressInfo).port);
  await Promise.all(servers.map((s) => new Promise((r) => s.close(r))));
  return ports;
}

// tauri-driver's ports, free ones rather than its fixed 4444/4445 defaults so
// concurrent runs (e.g. from two worktrees) don't collide. Picked once when the
// launcher loads the config and inherited (via the environment) by the workers
// that load it again; set either variable to choose a port yourself.
if (!process.env.RDM_E2E_DRIVER_PORT || !process.env.RDM_E2E_NATIVE_PORT) {
  const [driver, native] = await freePorts(2);
  process.env.RDM_E2E_DRIVER_PORT ??= String(driver);
  process.env.RDM_E2E_NATIVE_PORT ??= String(native);
}
const DRIVER_PORT = Number(process.env.RDM_E2E_DRIVER_PORT);
const NATIVE_PORT = Number(process.env.RDM_E2E_NATIVE_PORT);

// On Windows the app opens unfocused and behind other windows (it reads this
// variable through tauri-driver and msedgedriver), so a local run doesn't
// take over the desktop; set RDM_E2E_BACKGROUND=0 to watch it instead. Linux
// CI runs under xvfb and is left as it is.
if (isWindows) process.env.RDM_E2E_BACKGROUND ??= "1";

/** The WebdriverIO settings both runs share; each adds its specs and hooks. */
export const baseConfig = {
  runner: "local",
  hostname: "127.0.0.1",
  port: DRIVER_PORT,
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
} satisfies WebdriverIO.Config;

let fixture: ChildProcess | undefined;
let tauriDriver: ChildProcess | undefined;

/** Start the fixture server (with `args`) on a free port and resolve its base URL. */
export function startFixture(args: string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn(fixtureBinary, ["--port", "0", ...args], {
      stdio: ["ignore", "pipe", "inherit"],
    });
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

/** Start tauri-driver and wait until it accepts connections. */
export async function startTauriDriver(): Promise<void> {
  const driverArgs = ["--port", String(DRIVER_PORT), "--native-port", String(NATIVE_PORT)];
  if (process.env.MSEDGEDRIVER_PATH)
    driverArgs.push("--native-driver", process.env.MSEDGEDRIVER_PATH);
  tauriDriver = spawn(tauriDriverBinary, driverArgs, {
    stdio: ["ignore", "inherit", "inherit"],
  });
  tauriDriver.once("error", (e) => console.error(`tauri-driver failed to start: ${e}`));
  await waitForPort(DRIVER_PORT);
  console.log(`tauri-driver on port ${DRIVER_PORT} (native driver on ${NATIVE_PORT})`);
}

/** Stop tauri-driver and the fixture server. */
export function stopAll(): void {
  tauriDriver?.kill();
  fixture?.kill();
}
