// Minisign signatures as the Tauri updater uses them, on Node's built-in
// crypto only (Ed25519 and BLAKE2b-512), so checking a release needs no
// third-party code.
//
// Tauri wraps minisign's text files in base64: the updater public key is
// base64 of a minisign public key file, and each `.sig` is base64 of a
// minisign signature file. `tauri signer sign --app-version` puts
// `version:<v>` in the signature's trusted comment, which the signature
// covers.
//
// `verify` checks a release; `generateKey` and `sign` exist for tests.

import { createHash, createPublicKey, generateKeyPairSync, randomBytes, sign as edSign, verify as edVerify } from "node:crypto";

// DER prefix of an Ed25519 SubjectPublicKeyInfo; the raw 32-byte key follows.
const SPKI_ED25519 = Buffer.from("302a300506032b6570032100", "hex");

const unbase64 = (text) => Buffer.from(text.trim(), "base64");

/** The non-comment lines of a minisign file, after Tauri's base64 wrapper. */
function minisignLines(wrapped, what) {
  const text = unbase64(wrapped).toString("utf8");
  const lines = text.split(/\r?\n/).filter((l) => l.trim() !== "");
  if (!lines[0]?.startsWith("untrusted comment:")) throw new Error(`${what} is not a minisign file`);
  return lines;
}

/** `{ keyId, key }` from a Tauri updater public key. */
export function parsePublicKey(wrapped) {
  const raw = unbase64(minisignLines(wrapped, "public key")[1] ?? "");
  if (raw.length !== 42 || raw.subarray(0, 2).toString() !== "Ed") throw new Error("not an Ed25519 minisign public key");
  return {
    keyId: raw.subarray(2, 10),
    key: createPublicKey({ key: Buffer.concat([SPKI_ED25519, raw.subarray(10)]), format: "der", type: "spki" }),
  };
}

/**
 * Checks a Tauri `.sig` for `data` against a Tauri public key. Returns the
 * trusted comment's fields (e.g. `version`); throws if the signature doesn't
 * verify.
 */
export function verify(data, wrappedSig, wrappedPubkey) {
  const { keyId, key } = parsePublicKey(wrappedPubkey);
  const lines = minisignLines(wrappedSig, "signature");
  const sigRaw = unbase64(lines[1] ?? "");
  const trusted = lines[2] ?? "";
  const globalSig = unbase64(lines[3] ?? "");
  if (sigRaw.length !== 74 || !trusted.startsWith("trusted comment: ") || globalSig.length !== 64)
    throw new Error("malformed minisign signature");
  const algorithm = sigRaw.subarray(0, 2).toString();
  if (!sigRaw.subarray(2, 10).equals(keyId)) throw new Error("signed with a different key");
  const signature = sigRaw.subarray(10);
  const message = algorithm === "ED" ? createHash("blake2b512").update(data).digest() : algorithm === "Ed" ? data : null;
  if (!message) throw new Error(`unknown signature algorithm ${algorithm}`);
  if (!edVerify(null, message, key, signature)) throw new Error("signature doesn't match the file");
  const comment = trusted.slice("trusted comment: ".length);
  if (!edVerify(null, Buffer.concat([signature, Buffer.from(comment)]), key, globalSig))
    throw new Error("trusted comment signature doesn't match");
  return Object.fromEntries(
    comment
      .split("\t")
      .map((field) => field.split(/:(.*)/s))
      .filter((pair) => pair.length > 1)
      .map(([k, v]) => [k, v]),
  );
}

/** A throwaway keypair for tests: `{ publicKey, privateKey, keyId }`. */
export function generateKey() {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const raw = publicKey.export({ format: "der", type: "spki" }).subarray(SPKI_ED25519.length);
  const keyId = randomBytes(8);
  const file = `untrusted comment: minisign public key\n${Buffer.concat([Buffer.from("Ed"), keyId, raw]).toString("base64")}\n`;
  return { publicKey: Buffer.from(file).toString("base64"), privateKey, keyId };
}

/** A Tauri-style `.sig` for `data` with `version` in its trusted comment (tests only). */
export function sign(data, { privateKey, keyId }, version) {
  const signature = edSign(null, createHash("blake2b512").update(data).digest(), privateKey);
  const comment = `timestamp:0\tfile:test\tversion:${version}`;
  const globalSig = edSign(null, Buffer.concat([signature, Buffer.from(comment)]), privateKey);
  const file = [
    "untrusted comment: signature from tauri secret key",
    Buffer.concat([Buffer.from("ED"), keyId, signature]).toString("base64"),
    `trusted comment: ${comment}`,
    globalSig.toString("base64"),
    "",
  ].join("\n");
  return Buffer.from(file).toString("base64");
}
