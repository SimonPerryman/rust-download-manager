// Runs each screenshot spec in its own wdio run, so each gets a fresh app, data
// folder and downloads folder (see wdio.screenshots.conf.ts) and one spec's
// downloads never leak into another's shots. `--spec <file>` (repeatable) picks
// specs; with none, every screenshots/*.shots.ts runs, in name order.

import { spawnSync } from "node:child_process";
import { readdirSync } from "node:fs";
import path from "node:path";

const args = process.argv.slice(2);
const picked = [];
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--spec" && args[i + 1]) picked.push(args[++i]);
  else if (args[i].startsWith("--spec=")) picked.push(args[i].slice("--spec=".length));
  else {
    console.error(`unknown argument: ${args[i]}`);
    process.exit(2);
  }
}
const specs = picked.length
  ? picked
  : readdirSync("screenshots")
      .filter((f) => f.endsWith(".shots.ts"))
      .sort()
      .map((f) => `screenshots/${f}`);

const wdio = path.join("node_modules", "@wdio", "cli", "bin", "wdio.js");
for (const spec of specs) {
  const name = path.basename(spec).replace(/\.shots\.ts$/, "");
  console.log(`\n=== screenshots: ${name} ===`);
  const run = spawnSync(process.execPath, [wdio, "run", "wdio.screenshots.conf.ts", "--spec", spec], {
    stdio: "inherit",
    env: { ...process.env, RDM_SHOTS_NAME: name },
  });
  if (run.status !== 0) process.exit(run.status ?? 1);
}
