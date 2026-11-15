// Guard: every third-party `uses:` is on the repo's allowed-actions list.
// The repo setting "Allow specified actions" (Settings > Actions > General)
// rejects any other action with a `startup_failure` before a job runs, and PR
// CI never runs the release workflows, so nothing else would catch it.
// `.github/allowed-actions.txt` mirrors that setting. GitHub-owned actions
// (`actions/*`, `github/*`) and local `./` paths are always allowed.
//
// Usage: node scripts/guard-action-allowlist.mjs [workflowsDir] [allowlistFile]
//        node scripts/guard-action-allowlist.mjs --self-test

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const escapeRegExp = (s) => s.replace(/[.+?^$()|{}[\]\\]/g, "\\$&");
const globToRegExp = (pattern) =>
  new RegExp("^" + pattern.split("*").map(escapeRegExp).join(".*") + "$", "i");

export function parseAllowlist(text) {
  return text
    .split("\n")
    .map((l) => l.replace(/(^|\s)#.*$/, "").trim())
    .filter(Boolean);
}

/** Returns the offending `uses:` values in a workflow's text. */
export function disallowedUses(text, patterns) {
  const allowed = patterns.map(globToRegExp);
  const bad = [];
  text.split("\n").forEach((raw, i) => {
    const m = raw.replace(/(^|\s)#.*$/, "").match(/^\s*-?\s*uses:\s*["']?([^\s"']+)/);
    if (!m) return;
    const ref = m[1];
    if (ref.startsWith("./")) return;
    // `owner/repo/sub/path@ref` is matched as `owner/repo@ref`.
    const [path, version = ""] = ref.split("@");
    const [owner, repo] = path.split("/");
    if (/^(actions|github)$/i.test(owner)) return;
    const normalized = `${owner}/${repo}@${version}`;
    if (!allowed.some((re) => re.test(normalized))) bad.push({ line: i + 1, ref });
  });
  return bad;
}

function selfTest() {
  const patterns = parseAllowlist("# c\nfoo/bar@*\nbaz/qux@*  # trailing\n");
  const cases = [
    ["      - uses: actions/checkout@abc # v4", 0],
    ["        uses: github/codeql-action/init@abc", 0],
    ["    uses: ./.github/workflows/x.yml", 0],
    ["      - uses: foo/bar@abc", 0],
    ["      - uses: baz/qux/sub@abc", 0],
    ["      - uses: step-security/harden-runner@abc", 1],
    ["      - uses: foo/other@abc", 1],
    ["      # uses: evil/thing@abc", 0],
  ];
  let failed = 0;
  for (const [line, want] of cases) {
    const got = disallowedUses(line, patterns).length;
    if (got !== want) {
      console.error(`guard-action-allowlist self-test: expected ${want}, got ${got} for: ${line}`);
      failed++;
    }
  }
  if (failed) process.exit(1);
  console.log("guard-action-allowlist self-test: ok");
}

if (process.argv[2] === "--self-test") {
  selfTest();
} else {
  const dir = process.argv[2] ?? join("..", ".github", "workflows");
  const listFile = process.argv[3] ?? join("..", ".github", "allowed-actions.txt");
  const patterns = parseAllowlist(readFileSync(listFile, "utf8"));
  const violations = [];
  for (const name of readdirSync(dir).filter((n) => /\.ya?ml$/.test(n))) {
    const file = join(dir, name);
    for (const v of disallowedUses(readFileSync(file, "utf8"), patterns)) {
      violations.push({ file, ...v });
    }
  }
  if (violations.length) {
    for (const v of violations) {
      console.error(
        `guard-action-allowlist: third-party action not on the allowed list\n  ${relative(process.cwd(), v.file)}:${v.line}  uses: ${v.ref}\n  Fix: add its \`owner/repo@*\` pattern to .github/allowed-actions.txt AND to Settings > Actions > General > "Allow specified actions" before merging.\n`,
      );
    }
    process.exit(1);
  }
  console.log("guard-action-allowlist: ok");
}
