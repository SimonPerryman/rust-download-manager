// Check: the frontend's test hooks ship only in the e2e build. The fallback
// screen's trigger (RenderErrorProbe) is compiled in only under
// `vite build --mode e2e`; a release bundle must not contain it.
// - `absent` (after `pnpm build`): no file in dist names the hook's event;
// - `present` (after `pnpm build:e2e`): one does, which proves the check can
//   still see the hook (say, after a rename).
//
// Usage: node scripts/check-test-hooks.mjs absent|present [distDir]

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const PROBE = join("src", "components", "troubleshooting", "RenderErrorProbe.tsx");

const expect = process.argv[2];
const dist = process.argv[3] ?? "dist";
if (expect !== "absent" && expect !== "present") {
  console.error("usage: check-test-hooks.mjs absent|present [distDir]");
  process.exit(2);
}

const event = readFileSync(PROBE, "utf8").match(/RENDER_ERROR_EVENT = "([^"]+)"/)?.[1];
if (!event) {
  console.error(`check-test-hooks: no RENDER_ERROR_EVENT in ${PROBE}`);
  process.exit(1);
}

const files = readdirSync(dist, { recursive: true })
  .map(String)
  .filter((f) => /\.(js|html)$/.test(f));
const found = files.filter((f) => readFileSync(join(dist, f), "utf8").includes(event));

if (expect === "absent" && found.length > 0) {
  console.error(`check-test-hooks: the release bundle contains the test hook "${event}":`);
  for (const f of found) console.error(`  ${join(dist, f)}`);
  process.exit(1);
}
if (expect === "present" && found.length === 0) {
  console.error(`check-test-hooks: the e2e bundle lacks the test hook "${event}"`);
  process.exit(1);
}
console.log(`check-test-hooks: test hook ${expect} in ${dist}`);
