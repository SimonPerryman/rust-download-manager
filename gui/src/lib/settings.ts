// Pure helpers for the engine-backed Downloads settings form: render a stored
// rate back into the text the engine's `parse_rate` accepts, read the
// whole-number fields, and turn edited fields into a partial settings update.
// Validation of rates themselves stays in the engine.

import type { Settings } from "./types";

const UNITS: [suffix: string, bytes: number][] = [
  ["G", 1024 ** 3],
  ["M", 1024 ** 2],
  ["K", 1024],
];

/**
 * A bytes/s rate as editable text in the largest exact `K`/`M`/`G` unit
 * (`2097152` → `"2M"`), plain bytes otherwise; `null` (unlimited) → `""`.
 */
export function formatRate(bytesPerSec: number | null): string {
  if (bytesPerSec == null || bytesPerSec <= 0) return "";
  for (const [suffix, size] of UNITS) {
    if (bytesPerSec % size === 0) return `${bytesPerSec / size}${suffix}`;
  }
  return String(bytesPerSec);
}

/** A whole number typed into a count field, clamped to `[min, max]`; null if not a number. */
export function parseCount(text: string, min: number, max: number): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  return Math.min(max, Math.max(min, Number(trimmed)));
}

/** The form's editable fields, each a text draft. */
export type SettingsField = "concurrent" | "limit" | "connections";
export type SettingsDrafts = Record<SettingsField, string>;

/** A partial `set_settings` update: omitted fields are left unchanged. */
export interface SettingsPatch {
  maxConcurrent?: number;
  /** A rate string; blank = unlimited. */
  globalLimit?: string;
  maxConnections?: number;
}

// Mirrors the engine's clamp for connections; the concurrency ceiling is only
// a sanity bound for the text field (the engine just requires at least 1).
export const MAX_CONCURRENT = 99;
export const MAX_CONNECTIONS = 16;

const SETTINGS_KEY = {
  concurrent: "max_concurrent",
  limit: "global_limit",
  connections: "max_connections",
} as const satisfies Record<SettingsField, keyof Settings>;

/** The text each field shows for `s`. */
export function draftsFrom(s: Settings): SettingsDrafts {
  return {
    concurrent: String(s.max_concurrent),
    limit: formatRate(s.global_limit),
    connections: String(s.max_connections),
  };
}

/**
 * The update for one field's draft, or null if a count field isn't a whole
 * number. Rates are passed through for the engine to validate.
 */
export function fieldPatch(field: SettingsField, text: string): SettingsPatch | null {
  switch (field) {
    case "concurrent": {
      const n = parseCount(text, 1, MAX_CONCURRENT);
      return n == null ? null : { maxConcurrent: n };
    }
    case "connections": {
      const n = parseCount(text, 1, MAX_CONNECTIONS);
      return n == null ? null : { maxConnections: n };
    }
    case "limit":
      return { globalLimit: text.trim() };
  }
}

/**
 * A separate update per field whose draft differs from `saved`, skipping
 * counts that aren't whole numbers. Separate so the engine rejecting one
 * (e.g. a bad rate) can't sink the others.
 */
export function dirtyPatches(drafts: SettingsDrafts, saved: Settings): SettingsPatch[] {
  const current = draftsFrom(saved);
  return (Object.keys(drafts) as SettingsField[])
    .filter((field) => drafts[field] !== current[field])
    .map((field) => fieldPatch(field, drafts[field]))
    .filter((patch) => patch != null);
}

/**
 * `prev` with only `fields` taken from `applied`, so a late response for one
 * field can't roll back another field a newer response already set.
 */
export function mergeApplied(
  prev: Settings | null,
  applied: Settings,
  fields: SettingsField[],
): Settings {
  if (!prev) return applied;
  const next = { ...prev };
  for (const field of fields) {
    const key = SETTINGS_KEY[field];
    Object.assign(next, { [key]: applied[key] });
  }
  return next;
}
