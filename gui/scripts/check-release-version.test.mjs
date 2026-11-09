import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";
import { checkReleaseVersion } from "./check-release-version.mjs";

const REPO_ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

const roots = [];
afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

/** A minimal repo whose files all say `version`, with per-file overrides. */
function fixture(version, overrides = {}) {
  const files = {
    "Cargo.toml": `[workspace]\nmembers = ["gui/src-tauri"]\n\n[workspace.package]\nversion = "${version}"\n\n[package]\nname = "rdm"\nversion.workspace = true\n`,
    "gui/src-tauri/Cargo.toml": `[package]\nname = "rdm-gui"\nversion.workspace = true\n`,
    "Cargo.lock": `[[package]]\nname = "rdm"\nversion = "${version}"\n\n[[package]]\nname = "rdm-gui"\nversion = "${version}"\n`,
    "gui/package.json": JSON.stringify({ name: "rdm-gui", version }),
    "gui/src-tauri/tauri.conf.json": JSON.stringify({ productName: "rdm" }),
    "CHANGELOG.md": `# Changelog\n\n## Unreleased\n\n## ${version} - 2026-11-10\n\n- First.\n`,
    ...overrides,
  };
  const root = mkdtempSync(join(tmpdir(), "rdm-version-"));
  roots.push(root);
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), text);
  }
  return root;
}

describe("checkReleaseVersion", () => {
  it("passes when the tag and every version agree", () => {
    expect(checkReleaseVersion(fixture("1.2.3"), "v1.2.3")).toEqual([]);
  });

  it("fails on a tag that isn't vX.Y.Z", () => {
    expect(checkReleaseVersion(fixture("1.2.3"), "1.2.3")).toHaveLength(1);
  });

  it("fails when the tag differs from every file", () => {
    const errors = checkReleaseVersion(fixture("1.2.3"), "v1.2.4");
    expect(errors.join("\n")).toMatch(/Cargo\.toml \[workspace\.package\] version is 1\.2\.3/);
    expect(errors.join("\n")).toMatch(/gui\/package\.json/);
    expect(errors.join("\n")).toMatch(/CHANGELOG\.md/);
  });

  it("fails when only gui/package.json disagrees", () => {
    const root = fixture("1.2.3", { "gui/package.json": JSON.stringify({ version: "1.2.2" }) });
    expect(checkReleaseVersion(root, "v1.2.3")).toEqual(["gui/package.json version is 1.2.2, tag is 1.2.3"]);
  });

  it("fails when only the workspace version disagrees", () => {
    const root = fixture("1.2.3", {
      "Cargo.toml": `[workspace.package]\nversion = "1.2.2"\n\n[package]\nname = "rdm"\nversion.workspace = true\n`,
    });
    expect(checkReleaseVersion(root, "v1.2.3")).toEqual([
      "Cargo.toml [workspace.package] version is 1.2.2, tag is 1.2.3",
    ]);
  });

  it("finds the workspace version after an array value", () => {
    const root = fixture("1.2.3", {
      "Cargo.toml": `[workspace]\nmembers = ["gui/src-tauri"]\n\n[workspace.package]\nauthors = ["a"]\nversion = "1.2.3"\n\n[package]\nname = "rdm"\nversion.workspace = true\n`,
    });
    expect(checkReleaseVersion(root, "v1.2.3")).toEqual([]);
  });

  it("ignores a version in a later section", () => {
    const root = fixture("1.2.3", {
      "Cargo.toml": `[workspace.package]\nedition = "2021"\n\n[package]\nversion.workspace = true\n\n[other]\nversion = "1.2.3"\n`,
    });
    expect(checkReleaseVersion(root, "v1.2.3")).toEqual(["Cargo.toml has no [workspace.package] version"]);
  });

  it("fails when Cargo.lock is stale", () => {
    const root = fixture("1.2.3", {
      "Cargo.lock": `[[package]]\nname = "rdm"\nversion = "1.2.2"\n\n[[package]]\nname = "rdm-gui"\nversion = "1.2.3"\n`,
    });
    expect(checkReleaseVersion(root, "v1.2.3")).toHaveLength(1);
  });

  it("fails when a crate doesn't inherit the workspace version", () => {
    const root = fixture("1.2.3", { "gui/src-tauri/Cargo.toml": `[package]\nname = "rdm-gui"\nversion = "1.2.3"\n` });
    expect(checkReleaseVersion(root, "v1.2.3")).toHaveLength(1);
  });

  it("fails when tauri.conf.json pins its own version", () => {
    const root = fixture("1.2.3", { "gui/src-tauri/tauri.conf.json": JSON.stringify({ version: "1.2.3" }) });
    expect(checkReleaseVersion(root, "v1.2.3")).toHaveLength(1);
  });

  it("fails a stable tag whose CHANGELOG has only Unreleased", () => {
    const root = fixture("1.2.3", { "CHANGELOG.md": "# Changelog\n\n## Unreleased\n\n- New.\n" });
    expect(checkReleaseVersion(root, "v1.2.3")).toEqual(['CHANGELOG.md has no "## 1.2.3" heading']);
  });

  it("accepts a bracketed heading", () => {
    const root = fixture("1.2.3", { "CHANGELOG.md": "## [1.2.3] - 2026-11-10\n" });
    expect(checkReleaseVersion(root, "v1.2.3")).toEqual([]);
  });

  it("lets a pre-release use the Unreleased section", () => {
    const root = fixture("1.2.3-rc.1", { "CHANGELOG.md": "# Changelog\n\n## Unreleased\n\n- New.\n" });
    expect(checkReleaseVersion(root, "v1.2.3-rc.1")).toEqual([]);
  });

  it("exits non-zero from the command line on a mismatch, before any build can run", () => {
    const script = join(dirname(fileURLToPath(import.meta.url)), "check-release-version.mjs");
    const root = fixture("1.2.3");
    expect(spawnSync(process.execPath, [script, "v1.2.4", root]).status).toBe(1);
    expect(spawnSync(process.execPath, [script, "v1.2.3", root]).status).toBe(0);
  });

  it("passes for this repository at its own version", () => {
    const { version } = JSON.parse(readFileSync(join(REPO_ROOT, "gui", "package.json"), "utf8"));
    // Between releases the CHANGELOG has only Unreleased, which a stable tag rejects.
    const errors = checkReleaseVersion(REPO_ROOT, `v${version}`);
    expect(errors.filter((e) => !e.startsWith("CHANGELOG.md"))).toEqual([]);
  });
});
