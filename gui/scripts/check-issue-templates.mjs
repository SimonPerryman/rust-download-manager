// Check: the bug report form parses and asks for what a bug report needs. A
// broken form makes GitHub fall back to a blank issue without saying why.
// - bug_report.yml is valid YAML with a name, a description and a body;
// - every body item has a known type, and every input an id and a label;
// - version, os and steps are required, and there's a diagnostics textarea;
// - config.yml points security reports at the security policy.
//
// Usage: node scripts/check-issue-templates.mjs [templateDir]
//        (default: ../.github/ISSUE_TEMPLATE)

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parse } from "yaml";

const dir = process.argv[2] ?? join("..", ".github", "ISSUE_TEMPLATE");
const TYPES = new Set(["markdown", "input", "textarea", "dropdown", "checkboxes"]);
const REQUIRED = ["version", "os", "steps"];

const problems = [];
const load = (file) => {
  try {
    return parse(readFileSync(join(dir, file), "utf8"));
  } catch (e) {
    problems.push(`${file}: ${e.message}`);
    return null;
  }
};

const form = load("bug_report.yml");
if (form) {
  for (const key of ["name", "description"]) {
    if (typeof form[key] !== "string" || !form[key].trim()) {
      problems.push(`bug_report.yml: no ${key}`);
    }
  }
  const body = Array.isArray(form.body) ? form.body : [];
  if (body.length === 0) problems.push("bug_report.yml: no body");
  const byId = new Map();
  for (const [i, item] of body.entries()) {
    if (!TYPES.has(item?.type)) problems.push(`bug_report.yml: body[${i}] has type ${item?.type}`);
    if (item?.type === "markdown") continue;
    if (!item?.id) problems.push(`bug_report.yml: body[${i}] has no id`);
    if (!item?.attributes?.label) problems.push(`bug_report.yml: body[${i}] has no label`);
    if (item?.id) byId.set(item.id, item);
  }
  for (const id of REQUIRED) {
    if (byId.get(id)?.validations?.required !== true) {
      problems.push(`bug_report.yml: "${id}" is missing or not required`);
    }
  }
  if (byId.get("diagnostics")?.type !== "textarea") {
    problems.push('bug_report.yml: no "diagnostics" textarea');
  }
}

const config = load("config.yml");
if (config) {
  const links = Array.isArray(config.contact_links) ? config.contact_links : [];
  if (!links.some((l) => /\/security\/policy$/.test(l?.url ?? ""))) {
    problems.push("config.yml: no contact link to the security policy");
  }
}

if (problems.length > 0) {
  for (const p of problems) console.error(`check-issue-templates: ${p}`);
  process.exit(1);
}
console.log("check-issue-templates: ok");
