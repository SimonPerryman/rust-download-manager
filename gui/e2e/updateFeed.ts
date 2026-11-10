// A local release feed for the updater e2e: serves a `latest.json` and its
// payload signed with a throwaway key, never GitHub. The version it offers is
// read from a file a spec can rewrite, and `/stats` counts manifest requests,
// so a spec can tell whether the app checked.

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import http from "node:http";
import type net from "node:net";
import path from "node:path";
import { generateKey, sign } from "../scripts/minisign.mjs";

const repoRoot = path.resolve(import.meta.dirname, "../..");
const PAYLOAD = Buffer.from("not really rdm: the e2e update payload\n");
// Every installer kind, so whichever this platform is has an entry.
const PLATFORMS = [
  "windows-x86_64-nsis",
  "windows-x86_64-msi",
  "darwin-aarch64-app",
  "darwin-x86_64-app",
  "linux-x86_64-appimage",
  "linux-aarch64-appimage",
];

/** The app's version, from the workspace Cargo.toml (the GUI crate inherits it). */
export function appVersion(): string {
  const toml = readFileSync(path.join(repoRoot, "Cargo.toml"), "utf8");
  const match = /\[workspace\.package\][^[]*?\nversion = "([^"]+)"/.exec(toml);
  if (!match) throw new Error("no [workspace.package] version in Cargo.toml");
  return match[1];
}

/** A version newer than the app's (the next minor release). */
export function newerVersion(): string {
  const [major, minor] = appVersion().split(".").map(Number);
  return `${major}.${minor + 1}.0`;
}

/** The file whose contents are the version the feed offers. */
export const offerFile = (root: string) => path.join(root, "update-offer.txt");

/** The installer kind the app acts as (the debug build isn't installed from one). */
export const bundleKind = () =>
  process.platform === "win32" ? "nsis" : process.platform === "darwin" ? "app" : "appimage";

/**
 * Start the feed on loopback, offering the app's own version until a spec
 * rewrites `offerFile(root)`. Resolves with its base URL and public key.
 */
export async function startUpdateFeed(root: string): Promise<{ base: string; pubkey: string }> {
  const key = generateKey();
  const file = offerFile(root);
  writeFileSync(file, appVersion());
  let manifests = 0;
  let base = "";

  const server = http.createServer((req, res) => {
    const route = new URL(req.url ?? "/", "http://feed").pathname;
    if (route === "/latest.json") {
      manifests += 1;
      const version = existsSync(file) ? readFileSync(file, "utf8").trim() : appVersion();
      const signature = sign(PAYLOAD, key, version);
      const entry = { signature, url: `${base}/payload` };
      const manifest = {
        version,
        notes: "e2e",
        pub_date: "2026-01-01T00:00:00Z",
        platforms: Object.fromEntries(PLATFORMS.map((p) => [p, entry])),
      };
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify(manifest));
    } else if (route === "/payload") {
      res.writeHead(200, {
        "content-type": "application/octet-stream",
        "content-length": PAYLOAD.length,
      });
      res.end(PAYLOAD);
    } else if (route === "/stats") {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ manifests }));
    } else {
      res.writeHead(404);
      res.end();
    }
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  // The launcher waits on the runner, not on this server.
  server.unref();
  base = `http://127.0.0.1:${(server.address() as net.AddressInfo).port}`;
  return { base, pubkey: key.publicKey };
}
