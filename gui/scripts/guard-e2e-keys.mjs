// Guard: `browser.keys()` in the webview e2e specs may only send string
// literals without a repeated consecutive character, or `Key.*` constants.
// See "When an e2e spec types text" in AGENTS.md (Testing philosophy).
//
// Usage: node scripts/guard-e2e-keys.mjs [specDir]   (default: e2e/specs)

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const RULE =
  "browser.keys() sends only Key.* constants and string literals without a repeated consecutive character";
const FIX =
  "type the text with element send-keys instead: `await input.addValue(text)` (or setValue) — " +
  "WebKitGTK's driver drops repeated characters sent as key actions (AGENTS.md, Testing philosophy)";

const specDir = process.argv[2] ?? "e2e/specs";
const LITERAL = /(["'`])((?:\\.|(?!\1)[^\\])*)\1/g;

/** `src` with comments blanked out (same length, so offsets and lines still match). */
function stripComments(src) {
  const blank = (m) => m.replace(/[^\n]/g, " ");
  return src
    .replace(/\/\*[\s\S]*?\*\//g, blank)
    .replace(/(^|[\s;{}(),])\/\/[^\n]*/g, (m, lead) => lead + blank(m.slice(lead.length)));
}

/** The argument text of a call whose "(" is at `open`, or null if unbalanced. */
function callArgs(src, open) {
  let depth = 0;
  for (let i = open; i < src.length; i++) {
    if (src[i] === "(") depth++;
    else if (src[i] === ")" && --depth === 0) return src.slice(open + 1, i);
  }
  return null;
}

const violations = [];
for (const name of readdirSync(specDir).filter((f) => f.endsWith(".ts"))) {
  const file = join(specDir, name);
  const raw = readFileSync(file, "utf8");
  const src = stripComments(raw);
  for (const m of src.matchAll(/browser\.keys\s*\(/g)) {
    const open = m.index + m[0].length - 1;
    const line = src.slice(0, m.index).split("\n").length;
    const snippet = raw.split("\n")[line - 1].trim();
    const args = callArgs(src, open);
    if (args == null) continue;
    for (const [, , text] of args.matchAll(LITERAL)) {
      if (/(.)\1/.test(text)) {
        violations.push({ file, line, snippet, detail: `literal "${text}" repeats a character` });
      }
    }
    const rest = args.replace(LITERAL, "").replace(/\bKey\.\w+/g, "").replace(/[\s,[\]]/g, "");
    if (rest) {
      violations.push({ file, line, snippet, detail: `non-literal argument (\`${rest}\`) can't be checked` });
    }
  }
}

if (violations.length) {
  console.error(`guard-e2e-keys: ${RULE}\n`);
  for (const v of violations) {
    console.error(`  ${relative(process.cwd(), v.file)}:${v.line}  ${v.snippet}\n    ${v.detail}`);
  }
  console.error(`\nFix: ${FIX}`);
  process.exit(1);
}
console.log("guard-e2e-keys: ok");
