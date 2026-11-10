// Types for minisign.mjs, for the TypeScript that imports it (the e2e update feed).

import type { KeyObject } from "node:crypto";

export interface TestKey {
  /** The Tauri updater public key (base64 of a minisign public key file). */
  publicKey: string;
  privateKey: KeyObject;
  keyId: Buffer;
}

export function parsePublicKey(wrapped: string): { keyId: Buffer; key: KeyObject };
export function verify(data: Buffer, wrappedSig: string, wrappedPubkey: string): Record<string, string>;
export function generateKey(): TestKey;
export function sign(data: Buffer, key: TestKey, version: string): string;
