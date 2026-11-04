// Pure helpers for the engine-backed Downloads settings form: read the
// whole-number fields, show the stored global limit as a rate draft, and turn
// edited fields into a partial settings update. Beyond refusing what the rate
// box can't mean (see `composeRate`), validating rates stays in the engine.

import { composeRate, rateDraft, sameRate, type RateDraft } from "./rate";
import type { Settings } from "./types";

/** A whole number typed into a count field, clamped to `[min, max]`; null if not a number. */
export function parseCount(text: string, min: number, max: number): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  return Math.min(max, Math.max(min, Number(trimmed)));
}

/** The form's editable fields: two counts and the global limit. */
export type SettingsField = "concurrent" | "limit" | "connections";
export interface SettingsDrafts {
  concurrent: string;
  limit: RateDraft;
  connections: string;
}

/** A partial `set_settings` update: omitted fields are left unchanged. */
export interface SettingsPatch {
  maxConcurrent?: number;
  /** A rate string; blank = unlimited. */
  globalLimit?: string;
  maxConnections?: number;
}

/** One field's update, or why its draft can't be sent. */
export type FieldPatch = { patch: SettingsPatch } | { error: string };

// Mirrors the engine's clamp for connections; the concurrency ceiling is only
// a sanity bound for the text field (the engine just requires at least 1).
export const MAX_CONCURRENT = 99;
export const MAX_CONNECTIONS = 16;

const NOT_A_COUNT = "Enter a whole number.";

const SETTINGS_KEY = {
  concurrent: "max_concurrent",
  limit: "global_limit",
  connections: "max_connections",
} as const satisfies Record<SettingsField, keyof Settings>;

/** What each field shows for `s`. */
export function draftsFrom(s: Settings): SettingsDrafts {
  return {
    concurrent: String(s.max_concurrent),
    limit: rateDraft(s.global_limit),
    connections: String(s.max_connections),
  };
}

/** Whether `field`'s draft differs from what `saved` shows. */
export function isDirty(field: SettingsField, drafts: SettingsDrafts, saved: Settings): boolean {
  const current = draftsFrom(saved);
  return field === "limit"
    ? !sameRate(drafts.limit, current.limit)
    : drafts[field] !== current[field];
}

/**
 * The update for one field's draft. Counts must be whole numbers; a rate
 * draft is refused only for 0 or a lone "." and otherwise left to the engine.
 */
export function fieldPatch(field: SettingsField, drafts: SettingsDrafts): FieldPatch {
  switch (field) {
    case "concurrent": {
      const n = parseCount(drafts.concurrent, 1, MAX_CONCURRENT);
      return n == null ? { error: NOT_A_COUNT } : { patch: { maxConcurrent: n } };
    }
    case "connections": {
      const n = parseCount(drafts.connections, 1, MAX_CONNECTIONS);
      return n == null ? { error: NOT_A_COUNT } : { patch: { maxConnections: n } };
    }
    case "limit": {
      const r = composeRate(drafts.limit);
      return r.ok ? { patch: { globalLimit: r.rate } } : { error: r.error };
    }
  }
}

const FIELDS: SettingsField[] = ["concurrent", "limit", "connections"];

/**
 * A separate update per dirty field, skipping drafts that can't be sent.
 * Separate so the engine rejecting one (e.g. a bad rate) can't sink the
 * others.
 */
export function dirtyPatches(drafts: SettingsDrafts, saved: Settings): SettingsPatch[] {
  return FIELDS.filter((field) => isDirty(field, drafts, saved)).flatMap((field) => {
    const r = fieldPatch(field, drafts);
    return "patch" in r ? [r.patch] : [];
  });
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
