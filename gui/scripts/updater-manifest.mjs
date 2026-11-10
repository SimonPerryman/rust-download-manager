// Release step: write the in-app updater's latest.json for a tag from the
// signed GUI bundles, after checking every signature against the public key
// in tauri.conf.json. The release workflow runs it before the release is
// created, so a release never ships a manifest the app would reject.
//
// The directory holds the bundles and the `.sig` that `tauri signer sign
// --app-version` wrote next to each. One entry per installer kind, keyed
// `{os}-{arch}-{installer}` as the app looks them up, so an install is only
// ever updated by its own kind of installer (an .msi by the .msi). A .deb has
// no entry: it can't update in place.
//
// Usage: node gui/scripts/updater-manifest.mjs <tag> <bundlesDir> [repoRoot]
//        (writes <bundlesDir>/latest.json)

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { verify } from "./minisign.mjs";

const REPO = "SimonPerryman/rust-download-manager";

/** Each manifest key and the bundle file name pattern that fills it. */
export const ENTRIES = {
  "windows-x86_64-nsis": /_x64-setup\.exe$/,
  "windows-x86_64-msi": /_x64_[^/]*\.msi$/,
  "darwin-aarch64-app": /_aarch64\.app\.tar\.gz$/,
  "darwin-x86_64-app": /_x64\.app\.tar\.gz$/,
  "linux-x86_64-appimage": /\.AppImage$/,
};

/**
 * The manifest for `tag` from `files` (name → contents) and `sigs` (bundle
 * name → its `.sig` text), checked against `pubkey`. Returns
 * `{ manifest, errors }`; any error means it must not ship.
 */
export function buildManifest({ tag, files, sigs, pubkey, date = new Date() }) {
  const errors = [];
  const version = tag.replace(/^v/, "");
  const platforms = {};
  for (const [key, pattern] of Object.entries(ENTRIES)) {
    const names = Object.keys(files).filter((n) => pattern.test(n));
    if (names.length !== 1) {
      errors.push(`${key}: expected one bundle matching ${pattern}, found ${names.length ? names.join(", ") : "none"}`);
      continue;
    }
    const [name] = names;
    const signature = sigs[name];
    if (!signature) {
      errors.push(`${key}: ${name} has no .sig`);
      continue;
    }
    try {
      const fields = verify(files[name], signature, pubkey);
      if (fields.version !== version)
        errors.push(`${key}: ${name} is signed for version ${fields.version ?? "(none)"}, the tag is ${version}`);
    } catch (e) {
      errors.push(`${key}: ${name}: ${e.message}`);
      continue;
    }
    platforms[key] = {
      signature: signature.trim(),
      url: `https://github.com/${REPO}/releases/download/${tag}/${encodeURIComponent(name)}`,
    };
  }
  const manifest = {
    version,
    notes: `https://github.com/${REPO}/releases/tag/${tag}`,
    pub_date: date.toISOString().replace(/\.\d{3}Z$/, "Z"),
    platforms,
  };
  return { manifest, errors };
}

/** The updater public key `tauri.conf.json` ships, or an error if it's missing or a placeholder. */
export function configuredPubkey(root) {
  const conf = JSON.parse(readFileSync(join(root, "gui", "src-tauri", "tauri.conf.json"), "utf8"));
  const pubkey = conf.plugins?.updater?.pubkey ?? "";
  if (!pubkey || /placeholder/i.test(pubkey))
    throw new Error("tauri.conf.json has no update public key (plugins.updater.pubkey is missing or a placeholder)");
  return pubkey;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [tag, dir, root = join(dirname(fileURLToPath(import.meta.url)), "..", "..")] = process.argv.slice(2);
  try {
    const names = readdirSync(dir);
    const files = Object.fromEntries(
      names.filter((n) => !n.endsWith(".sig") && n !== "latest.json").map((n) => [n, readFileSync(join(dir, n))]),
    );
    const sigs = Object.fromEntries(
      names.filter((n) => n.endsWith(".sig")).map((n) => [n.slice(0, -4), readFileSync(join(dir, n), "utf8")]),
    );
    const { manifest, errors } = buildManifest({ tag, files, sigs, pubkey: configuredPubkey(root) });
    if (errors.length) {
      for (const e of errors) console.error(`updater-manifest: ${e}`);
      process.exit(1);
    }
    writeFileSync(join(dir, "latest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
    console.log(`updater-manifest: ${Object.keys(manifest.platforms).length} entries verified for ${tag}`);
  } catch (e) {
    console.error(`updater-manifest: ${e.message}`);
    process.exit(1);
  }
}
