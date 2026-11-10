import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { generateKey, parsePublicKey, sign, verify } from "./minisign.mjs";
import { buildManifest, configuredPubkey, ENTRIES } from "./updater-manifest.mjs";

// Made once with `tauri signer generate` (a throwaway key, never used for a
// release) and `tauri signer sign --app-version 0.3.1`: proves the checker
// reads what the real tool writes.
const TAURI_PUBKEY =
  "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDdCNUZEMzE5QkVBOTFCOUIKUldTYkc2bStHZE5mZTB0MGs4WURPcUhJcjBzRmFkbGg0WkFQVURFajJKeW9mcVU4RTNrbVl5dUoK";
const TAURI_SIG =
  "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTYkc2bStHZE5mZTh5Y0pNbUgzRDRKUkFBK05ZZ21CSHFxdmppdnE5MXQ1aTA5bjE3R2l3SERIS0RQemhlQ05iQmp0QkJwZi8razlhUTZsY1pTbFFKdEJ0UGNXd1U4MGdnPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0MzM1OTgzCWZpbGU6cGF5bG9hZC5iaW4JdmVyc2lvbjowLjMuMQpVaERhZXpOcGRIblhrWUxkNEFTWjNpeXNTeUwxcmlaQ0YrSHdDbFNzTElqeFpoZFVhUHpWUjJFU0tiRWJlUlBuWU5YM3BSdFdUK0pNSEVHTWJySkZBQT09Cg==";
const TAURI_PAYLOAD = Buffer.from("rdm updater test payload\n");

const BUNDLES = [
  "rdm_0.3.1_x64-setup.exe",
  "rdm_0.3.1_x64_en-US.msi",
  "rdm_0.3.1_aarch64.app.tar.gz",
  "rdm_0.3.1_x64.app.tar.gz",
  "rdm_0.3.1_amd64.AppImage",
  "rdm_0.3.1_amd64.deb",
  "rdm_0.3.1_aarch64.dmg",
];

/** Every bundle, each signed by `key` for `version` (default 0.3.1). */
function release(key, { version = "0.3.1", skip = [] } = {}) {
  const files = Object.fromEntries(BUNDLES.map((n) => [n, Buffer.from(`contents of ${n}`)]));
  const sigs = Object.fromEntries(
    BUNDLES.filter((n) => !skip.includes(n)).map((n) => [n, sign(files[n], key, version)]),
  );
  return { files, sigs };
}

describe("minisign verify", () => {
  it("accepts a signature written by tauri signer sign", () => {
    expect(verify(TAURI_PAYLOAD, TAURI_SIG, TAURI_PUBKEY).version).toBe("0.3.1");
  });

  it("rejects a changed file", () => {
    expect(() => verify(Buffer.from("tampered\n"), TAURI_SIG, TAURI_PUBKEY)).toThrow(/doesn't match/);
  });

  it("rejects another key's signature", () => {
    const other = generateKey();
    expect(() => verify(TAURI_PAYLOAD, TAURI_SIG, other.publicKey)).toThrow(/different key/);
  });

  it("round-trips its own test signatures", () => {
    const key = generateKey();
    expect(verify(Buffer.from("x"), sign(Buffer.from("x"), key, "1.2.3"), key.publicKey).version).toBe("1.2.3");
  });
});

describe("buildManifest", () => {
  const date = new Date("2026-11-10T12:00:00.123Z");

  it("writes one verified entry per installer kind, and none for .deb", () => {
    const key = generateKey();
    const { manifest, errors } = buildManifest({ tag: "v0.3.1", ...release(key), pubkey: key.publicKey, date });
    expect(errors).toEqual([]);
    expect(manifest.version).toBe("0.3.1");
    expect(manifest.pub_date).toBe("2026-11-10T12:00:00Z");
    expect(manifest.notes).toBe("https://github.com/SimonPerryman/rust-download-manager/releases/tag/v0.3.1");
    expect(Object.keys(manifest.platforms).sort()).toEqual(Object.keys(ENTRIES).sort());
    expect(manifest.platforms["windows-x86_64-msi"].url).toBe(
      "https://github.com/SimonPerryman/rust-download-manager/releases/download/v0.3.1/rdm_0.3.1_x64_en-US.msi",
    );
    expect(JSON.stringify(manifest)).not.toMatch(/\.deb|\.dmg/);
  });

  it("fails on bundles signed with a different key", () => {
    const signer = generateKey();
    const trusted = generateKey();
    const { errors } = buildManifest({ tag: "v0.3.1", ...release(signer), pubkey: trusted.publicKey, date });
    expect(errors).toHaveLength(Object.keys(ENTRIES).length);
    expect(errors[0]).toMatch(/different key/);
  });

  it("fails on a signature bound to another version", () => {
    const key = generateKey();
    const { errors } = buildManifest({ tag: "v0.3.1", ...release(key, { version: "0.3.0" }), pubkey: key.publicKey, date });
    expect(errors[0]).toMatch(/signed for version 0\.3\.0, the tag is 0\.3\.1/);
  });

  it("fails on a missing bundle or signature", () => {
    const key = generateKey();
    const { files, sigs } = release(key, { skip: ["rdm_0.3.1_amd64.AppImage"] });
    delete files["rdm_0.3.1_x64.app.tar.gz"];
    const { errors } = buildManifest({ tag: "v0.3.1", files, sigs, pubkey: key.publicKey, date });
    expect(errors).toEqual([
      expect.stringMatching(/^darwin-x86_64-app: expected one bundle/),
      expect.stringMatching(/^linux-x86_64-appimage: .* has no \.sig/),
    ]);
  });
});

describe("configuredPubkey", () => {
  const roots = [];
  afterEach(() => {
    for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
  });
  const repo = (pubkey) => {
    const root = mkdtempSync(join(tmpdir(), "rdm-pubkey-"));
    roots.push(root);
    mkdirSync(join(root, "gui", "src-tauri"), { recursive: true });
    writeFileSync(join(root, "gui", "src-tauri", "tauri.conf.json"), JSON.stringify({ plugins: { updater: { pubkey } } }));
    return root;
  };

  it("refuses the placeholder", () => {
    expect(() => configuredPubkey(repo("PLACEHOLDER: the update public key goes here"))).toThrow(/placeholder/);
  });

  it("returns a real key", () => {
    expect(configuredPubkey(repo(TAURI_PUBKEY))).toBe(TAURI_PUBKEY);
  });

  it("ships a valid minisign public key", () => {
    const root = join(import.meta.dirname, "..", "..");
    expect(() => parsePublicKey(configuredPubkey(root))).not.toThrow();
  });
});
