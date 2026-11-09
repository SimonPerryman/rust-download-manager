// Guard: CI pins everything it runs on, so an upstream change (a `-latest`
// image migrating, a moved action tag) can neither break nor compromise a
// build unannounced. Rule lives in AGENTS.md (Toolchain).
// - runner images are exact, never `*-latest` (under any key).
// - `uses:` pins a full 40-hex commit SHA (with a `# vX.Y.Z` comment).
// - `node-version` is an exact x.y.z.
// - `cargo install` passes `--version`.
// - `tool:` (taiki-e/install-action) names an exact `name@x.y.z` per tool.
//
// Usage: node scripts/guard-ci-pins.mjs [workflowsDir]   (default: ../.github/workflows)

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const dir = process.argv[2] ?? join("..", ".github", "workflows");

const CHECKS = [
  {
    // Any key, not just `runs-on`/`os:`: matrix entries and label lists name images too.
    test: (line) => /\b(ubuntu|windows|macos)-latest\b/.test(line),
    rule: "runner image is `*-latest`",
    fix: "name the exact image, e.g. `ubuntu-24.04` or `windows-2025-vs2026`",
  },
  {
    test: (line) => {
      const m = line.match(/^\s*-?\s*uses:\s*([^\s#]+)/);
      return m && !m[1].startsWith("./") && !/@[0-9a-f]{40}$/.test(m[1]);
    },
    rule: "action not pinned to a commit SHA",
    fix: "use `owner/repo@<40-hex sha> # vX.Y.Z` (resolve with `git ls-remote --tags`)",
  },
  {
    test: (line) => {
      const m = line.match(/^\s*node-version:\s*["']?([^\s"'#]+)/);
      return m && !/^\d+\.\d+\.\d+$/.test(m[1]);
    },
    rule: "node-version is not an exact x.y.z",
    fix: "pin the full version, e.g. `20.20.2`",
  },
  {
    test: (line) => /\bcargo install\b/.test(line) && !/--version\b/.test(line),
    rule: "cargo install without --version",
    fix: "add `--version X.Y.Z --locked`",
  },
  {
    test: (line) => {
      const m = line.match(/^\s*tool:\s*["']?([^"'#]*)/);
      return m && m[1].split(",").some((t) => !/^[\w-]+@\d+\.\d+\.\d+$/.test(t.trim()));
    },
    rule: "install-action `tool:` without an exact version",
    fix: "pin each tool, e.g. `tool: cargo-nextest@0.9.151`",
  },
];

const violations = [];
for (const name of readdirSync(dir).filter((n) => /\.ya?ml$/.test(n))) {
  const file = join(dir, name);
  readFileSync(file, "utf8")
    .split("\n")
    .forEach((raw, i) => {
      const line = raw.replace(/(^|\s)#.*$/, "");
      for (const check of CHECKS) {
        if (check.test(line)) violations.push({ check, file, line: i + 1, snippet: raw.trim() });
      }
    });
}

if (violations.length) {
  for (const v of violations) {
    console.error(`guard-ci-pins: ${v.check.rule}\n  ${relative(process.cwd(), v.file)}:${v.line}  ${v.snippet}\n  Fix: ${v.check.fix}\n`);
  }
  process.exit(1);
}
console.log("guard-ci-pins: ok");
