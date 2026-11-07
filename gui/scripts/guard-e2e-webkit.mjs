// Guard: e2e calls that WebKitGTK's driver (the Linux CI job) gets wrong.
// - `browser.keys()` may only send `Key.*` constants and string literals
//   without a repeated consecutive character ("When an e2e spec types text").
// - `browser.keys()` may not send `Key.Shift` ("When an e2e spec needs
//   Shift+key").
// - `waitForExist()` may not set `reverse` ("When an e2e spec waits for an
//   element to disappear").
// All three rules live in AGENTS.md (Testing philosophy).
//
// Usage: node scripts/guard-e2e-webkit.mjs [e2eDir]   (default: e2e)

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const CHECKS = {
  keys: {
    rule: "browser.keys() sends only Key.* constants and string literals without a repeated consecutive character",
    fix:
      "type the text with element send-keys instead: `await input.addValue(text)` (or setValue) — " +
      "WebKitGTK's driver drops repeated characters sent as key actions " +
      '(AGENTS.md, Testing philosophy: "When an e2e spec types text")',
  },
  shift: {
    rule: "browser.keys() never sends Key.Shift",
    fix:
      "dispatch the key event with the modifier set instead: " +
      '`document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", shiftKey: true, bubbles: true }))` ' +
      "via browser.execute — WebKitGTK's driver drops the Shift (AGENTS.md, Testing philosophy: " +
      '"When an e2e spec needs Shift+key")',
  },
  reverse: {
    rule: "waitForExist() never sets `reverse`",
    fix:
      "use `waitForGone(selector, timeoutMsg?)` from gui/e2e/helpers.ts — WebKitGTK's driver throws " +
      "a stale-element error when the element detaches mid-check (AGENTS.md, Testing philosophy: " +
      '"When an e2e spec waits for an element to disappear")',
  },
};

const e2eDir = process.argv[2] ?? "e2e";
const LITERAL = /(["'`])((?:\\.|(?!\1)[^\\])*)\1/g;

/** Every `.ts` file under `dir`, skipping node_modules. */
function tsFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return entry.name === "node_modules" ? [] : tsFiles(path);
    return entry.name.endsWith(".ts") ? [path] : [];
  });
}

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

/** Each call to `pattern` (ending in "(") in `src`, with its line number and argument text. */
function* calls(src, pattern) {
  for (const m of src.matchAll(pattern)) {
    const args = callArgs(src, m.index + m[0].length - 1);
    if (args != null) yield { line: src.slice(0, m.index).split("\n").length, args };
  }
}

const violations = [];
for (const file of tsFiles(e2eDir)) {
  const raw = readFileSync(file, "utf8");
  const src = stripComments(raw);
  const report = (check, line, detail) =>
    violations.push({ check, file, line, snippet: raw.split("\n")[line - 1].trim(), detail });

  for (const { line, args } of calls(src, /browser\.keys\s*\(/g)) {
    for (const [, , text] of args.matchAll(LITERAL)) {
      if (/(.)\1/.test(text)) report("keys", line, `literal "${text}" repeats a character`);
    }
    const rest = args.replace(LITERAL, "").replace(/\bKey\.\w+/g, "").replace(/[\s,[\]]/g, "");
    if (rest) report("keys", line, `non-literal argument (\`${rest}\`) can't be checked`);
    if (/\bKey\.Shift\b/.test(args)) report("shift", line, "sends Key.Shift (a Shift chord)");
  }

  for (const { line, args } of calls(src, /\bwaitForExist\s*\(/g)) {
    // Literals blanked so a timeoutMsg mentioning "reverse" doesn't count.
    if (/\breverse\b/.test(args.replace(LITERAL, '""'))) {
      report("reverse", line, "sets `reverse` (waits for the element to disappear)");
    }
  }
}

if (violations.length) {
  for (const check of Object.keys(CHECKS)) {
    const hits = violations.filter((v) => v.check === check);
    if (!hits.length) continue;
    console.error(`guard-e2e-webkit: ${CHECKS[check].rule}\n`);
    for (const v of hits) {
      console.error(`  ${relative(process.cwd(), v.file)}:${v.line}  ${v.snippet}\n    ${v.detail}`);
    }
    console.error(`\nFix: ${CHECKS[check].fix}\n`);
  }
  process.exit(1);
}
console.log("guard-e2e-webkit: ok");
