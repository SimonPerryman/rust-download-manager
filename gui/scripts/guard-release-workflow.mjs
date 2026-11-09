// Guard: the release workflow can publish and push, so it runs only on a tag
// push and only its release job can write. A pull_request trigger would run
// release code on a PR, and a broad `write` would hand the token's write
// access to every build script and dependency.
// - release.yml triggers on `push: tags` only;
// - no top-level `write` permission (and top-level permissions are set);
// - a job-level `write` only where allowed (host: contents);
// - every workflow release.yml calls (`uses: ./.github/workflows/...`) is
//   `workflow_call`-only, grants no `write` and calls no workflow itself;
// - no job passes `secrets:` (a called workflow gets none of the repo's).
//
// Usage: node scripts/guard-release-workflow.mjs [workflowsDir]   (default: ../.github/workflows)
//        node scripts/guard-release-workflow.mjs --self-test     (mutated copies must fail)

import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const RELEASE = "release.yml";
// The one job that creates the release and uploads its assets.
const ALLOWED_WRITE = { host: ["contents"] };

/** Lines with comments and blank lines dropped, each with its indent. */
function lines(text) {
  return text
    .split(/\r?\n/)
    .map((raw) => raw.replace(/(^|\s)#.*$/, "").trimEnd())
    .filter((line) => line.trim() !== "")
    .map((line) => ({ indent: line.length - line.trimStart().length, text: line.trim() }));
}

/** The `key: value` children of the block starting at index `at`. */
function children(ls, at) {
  const out = [];
  const indent = ls[at].indent;
  let childIndent;
  for (let i = at + 1; i < ls.length && ls[i].indent > indent; i++) {
    childIndent ??= ls[i].indent;
    if (ls[i].indent !== childIndent) continue;
    const m = /^-?\s*["']?([\w-]+)["']?\s*:\s*(.*)$/.exec(ls[i].text);
    if (m) out.push({ key: m[1], value: m[2].replace(/^["']|["']$/g, ""), at: i });
  }
  return out;
}

function topLevel(ls) {
  return children([{ indent: -1, text: "" }, ...ls], 0).map((c) => ({ ...c, at: c.at - 1 }));
}

/** Violations for one permissions block (`value` inline, or its children). */
function permissionViolations(ls, entry, where, allowed = []) {
  if (entry.value) {
    return /write/.test(entry.value) ? [`${where}: permissions "${entry.value}" grants write`] : [];
  }
  return children(ls, entry.at)
    .filter((p) => p.value === "write" && !allowed.includes(p.key))
    .map((p) => `${where}: grants ${p.key}: write`);
}

/** Returns the rule violations in one workflow file's text. */
export function checkWorkflow(text, { reusable }) {
  const ls = lines(text);
  const top = topLevel(ls);
  const errors = [];

  const on = top.find((t) => t.key === "on" || t.key === "true");
  const triggers = on && !on.value ? children(ls, on.at) : [];
  if (reusable) {
    if (!on || on.value || triggers.map((t) => t.key).join() !== "workflow_call")
      errors.push("trigger: a called workflow must have only `on: workflow_call`");
  } else {
    const push = triggers.find((t) => t.key === "push");
    const pushKeys = push && !push.value ? children(ls, push.at).map((c) => c.key) : [];
    if (!on || on.value || triggers.length !== 1 || !push || pushKeys.join() !== "tags")
      errors.push(
        `trigger: only \`on: push: tags\` is allowed (found ${on?.value || triggers.map((t) => t.key).join(", ") || "none"})`,
      );
  }

  const topPermissions = top.find((t) => t.key === "permissions");
  if (!topPermissions) errors.push("top level: set `permissions: contents: read`");
  else errors.push(...permissionViolations(ls, topPermissions, "top level"));

  const jobs = top.find((t) => t.key === "jobs");
  for (const job of jobs ? children(ls, jobs.at) : []) {
    const props = children(ls, job.at);
    const permissions = props.find((p) => p.key === "permissions");
    const allowed = reusable ? [] : (ALLOWED_WRITE[job.key] ?? []);
    if (permissions) errors.push(...permissionViolations(ls, permissions, `job ${job.key}`, allowed));
    if (props.some((p) => p.key === "secrets")) errors.push(`job ${job.key}: passes \`secrets:\` to a called workflow`);
    const uses = props.find((p) => p.key === "uses");
    if (reusable && uses) errors.push(`job ${job.key}: a called workflow must not call another (${uses.value})`);
  }
  return errors;
}

/** Violations across release.yml and the workflows it calls. */
export function checkReleaseWorkflows(dir) {
  const release = readFileSync(join(dir, RELEASE), "utf8");
  const errors = checkWorkflow(release, { reusable: false }).map((e) => `${RELEASE}: ${e}`);
  for (const m of release.matchAll(/^\s*uses:\s*["']?\.\/\.github\/workflows\/([\w.-]+)/gm)) {
    const file = join(dir, m[1]);
    if (!existsSync(file)) {
      errors.push(`${RELEASE}: calls missing workflow ${m[1]}`);
      continue;
    }
    errors.push(...checkWorkflow(readFileSync(file, "utf8"), { reusable: true }).map((e) => `${m[1]}: ${e}`));
  }
  return errors;
}

/** Mutations of release.yml the guard must reject; returns the ones it let through. */
export function selfTest(dir) {
  const release = readFileSync(join(dir, RELEASE), "utf8");
  const mutations = {
    "a pull_request trigger": release.replace(/^on:\n/m, "on:\n  pull_request:\n"),
    "a workflow_dispatch trigger": release.replace(/^on:\n/m, "on:\n  workflow_dispatch:\n"),
    "top-level contents: write": release.replace(/^permissions:\n {2}contents: read/m, "permissions:\n  contents: write"),
    "top-level write-all": release.replace(/^permissions:\n {2}contents: read/m, "permissions: write-all"),
    "no top-level permissions": release.replace(/^permissions:\n {2}contents: read\n/m, ""),
    "write on a build job": release.replace(
      /^ {2}build-global-artifacts:\n/m,
      "  build-global-artifacts:\n    permissions:\n      contents: write\n",
    ),
    "secrets on a custom job": release.replace(/^( {4}uses: \.\/\.github\/workflows\/release-gui\.yml)$/m, "$1\n    secrets: inherit"),
    "packages: write on the host job": release.replace(/^( {4}permissions:\n {6}contents: write)/m, "$1\n      packages: write"),
  };
  const escaped = [];
  if (checkWorkflow(release, { reusable: false }).length) escaped.push("(the real release.yml fails)");
  for (const [name, text] of Object.entries(mutations)) {
    if (text === release) escaped.push(`${name} (mutation didn't apply)`);
    else if (!checkWorkflow(text, { reusable: false }).length) escaped.push(name);
  }
  return escaped;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const selfTestMode = process.argv[2] === "--self-test";
  const dir = (selfTestMode ? process.argv[3] : process.argv[2]) ?? join("..", ".github", "workflows");
  if (selfTestMode) {
    const escaped = selfTest(dir);
    if (escaped.length) {
      for (const e of escaped) console.error(`guard-release-workflow self-test: not caught: ${e}`);
      process.exit(1);
    }
    console.log("guard-release-workflow self-test: ok");
  } else {
    const errors = checkReleaseWorkflows(dir);
    if (errors.length) {
      for (const e of errors) console.error(`guard-release-workflow: ${e}`);
      process.exit(1);
    }
    console.log("guard-release-workflow: ok");
  }
}
