// Release check: a tag vX.Y.Z (or vX.Y.Z-rc.N) releases exactly that version,
// so the release workflow runs this before any build. It fails unless the tag,
// the workspace version (inherited by both crates), Cargo.lock, gui/package.json
// and a CHANGELOG.md heading agree.
//
// A stable tag needs a `## X.Y.Z` heading (optionally `[X.Y.Z]` and
// ` - <date>`). A pre-release tag also accepts `## Unreleased`, which is what
// dist falls back to for its release notes, so an rc needs no changelog edit.
//
// Usage: node gui/scripts/check-release-version.mjs <tag> [repoRoot]   (default: this repo)

import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const CRATES = { rdm: "Cargo.toml", "rdm-gui": join("gui", "src-tauri", "Cargo.toml") };

/** Returns one message per disagreement; empty when the tag may be released. */
export function checkReleaseVersion(root, tag) {
  const errors = [];
  const read = (path) => readFileSync(join(root, path), "utf8");

  const tagMatch = /^v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)$/.exec(tag ?? "");
  if (!tagMatch) return [`tag "${tag}" is not vX.Y.Z or vX.Y.Z-<pre-release>`];
  const version = tagMatch[1];
  const prerelease = version.includes("-");

  // Bounded by the next `[section]` header, so array values before `version` are fine.
  const workspace = /^\[workspace\.package\]\s*$(?:(?!^\[)[\s\S])*?^version\s*=\s*"([^"]+)"/m.exec(read("Cargo.toml"));
  if (!workspace) errors.push("Cargo.toml has no [workspace.package] version");
  else if (workspace[1] !== version)
    errors.push(`Cargo.toml [workspace.package] version is ${workspace[1]}, tag is ${version}`);

  for (const [crate, manifest] of Object.entries(CRATES)) {
    if (!/^version\.workspace\s*=\s*true\s*$/m.test(read(manifest)))
      errors.push(`${manifest} (${crate}) must set version.workspace = true`);
  }

  const lock = read("Cargo.lock");
  for (const crate of Object.keys(CRATES)) {
    const entry = new RegExp(`^name = "${crate}"\\r?\\nversion = "([^"]+)"`, "m").exec(lock);
    if (!entry) errors.push(`Cargo.lock has no entry for ${crate}`);
    else if (entry[1] !== version)
      errors.push(`Cargo.lock has ${crate} ${entry[1]}, tag is ${version} (run cargo update -w)`);
  }

  const pkg = JSON.parse(read(join("gui", "package.json"))).version;
  if (pkg !== version) errors.push(`gui/package.json version is ${pkg}, tag is ${version}`);

  if ("version" in JSON.parse(read(join("gui", "src-tauri", "tauri.conf.json"))))
    errors.push("gui/src-tauri/tauri.conf.json sets version; drop it so Tauri uses the crate's");

  const headings = [...read("CHANGELOG.md").matchAll(/^##\s+\[?([^\]\s]+)\]?(?:\s+-\s+.*)?\s*$/gm)].map(
    (m) => m[1],
  );
  const accepted = prerelease ? [version, "Unreleased"] : [version];
  if (!headings.some((h) => accepted.includes(h)))
    errors.push(`CHANGELOG.md has no "## ${accepted.join('" or "## ')}" heading`);

  return errors;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [tag, root = join(dirname(fileURLToPath(import.meta.url)), "..", "..")] = process.argv.slice(2);
  const errors = checkReleaseVersion(root, tag);
  if (errors.length) {
    for (const e of errors) console.error(`check-release-version: ${e}`);
    process.exit(1);
  }
  console.log(`check-release-version: ${tag} ok`);
}
