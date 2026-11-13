// Pure helpers for the engine-backed Downloads settings form: read the
// whole-number fields, show the stored global limit as a rate draft, and turn
// edited fields into a partial settings update. Beyond refusing what the rate
// box can't mean (see `composeRate`) and a blank custom proxy, validating
// rates and the proxy URL stays in the engine.

import { composeRate, rateDraft, sameRate, type RateDraft } from "./rate";
import type { ProxyModeName, ProxySetting, Settings } from "./types";

/** A whole number typed into a count field, clamped to `[min, max]`; null if not a number. */
export function parseCount(text: string, min: number, max: number): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  return Math.min(max, Math.max(min, Number(trimmed)));
}

/** The form's editable fields: two counts, the global limit and the proxy. */
export type SettingsField = "concurrent" | "limit" | "connections" | "proxy";
export interface SettingsDrafts {
  concurrent: string;
  limit: RateDraft;
  connections: string;
  /** The proxy's mode, and the URL used when it's custom. */
  proxyMode: ProxyModeName;
  proxy: string;
}

/** A partial `set_settings` update: omitted fields are left unchanged. */
export interface SettingsPatch {
  maxConcurrent?: number;
  /** A rate string; blank = unlimited. */
  globalLimit?: string;
  maxConnections?: number;
  /** A custom URL's masked password keeps the saved one. */
  proxy?: ProxySetting;
}

/** One field's update, or why its draft can't be sent. */
export type FieldPatch = { patch: SettingsPatch } | { error: string };

// Mirrors the engine's clamp for connections; the concurrency ceiling is only
// a sanity bound for the text field (the engine just requires at least 1).
export const MAX_CONCURRENT = 99;
export const MAX_CONNECTIONS = 16;

const NOT_A_COUNT = "Enter a whole number.";
const NO_PROXY_URL = "Enter the proxy's URL, or pick another option.";

const SETTINGS_KEY = {
  concurrent: "max_concurrent",
  limit: "global_limit",
  connections: "max_connections",
  proxy: "proxy",
} as const satisfies Record<SettingsField, keyof Settings>;

/** What each field shows for `s`. */
export function draftsFrom(s: Settings): SettingsDrafts {
  return {
    concurrent: String(s.max_concurrent),
    limit: rateDraft(s.global_limit),
    connections: String(s.max_connections),
    proxyMode: s.proxy.mode,
    proxy: s.proxy.mode === "custom" ? s.proxy.url : "",
  };
}

/**
 * `drafts` with `field` showing what the engine `applied`. A proxy mode
 * other than custom keeps the URL draft, so switching back finds it there.
 */
export function appliedDrafts(
  field: SettingsField,
  drafts: SettingsDrafts,
  applied: Settings,
): SettingsDrafts {
  const shown = draftsFrom(applied);
  if (field !== "proxy") return { ...drafts, [field]: shown[field] };
  return {
    ...drafts,
    proxyMode: shown.proxyMode,
    proxy: shown.proxyMode === "custom" ? shown.proxy : drafts.proxy,
  };
}

/** Whether `field`'s draft differs from what `saved` shows. */
export function isDirty(field: SettingsField, drafts: SettingsDrafts, saved: Settings): boolean {
  const current = draftsFrom(saved);
  if (field === "limit") return !sameRate(drafts.limit, current.limit);
  if (field === "proxy") {
    return (
      drafts.proxyMode !== current.proxyMode ||
      (drafts.proxyMode === "custom" && drafts.proxy.trim() !== current.proxy)
    );
  }
  return drafts[field] !== current[field];
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
    case "proxy": {
      if (drafts.proxyMode !== "custom") return { patch: { proxy: { mode: drafts.proxyMode } } };
      const url = drafts.proxy.trim();
      return url ? { patch: { proxy: { mode: "custom", url } } } : { error: NO_PROXY_URL };
    }
  }
}

const FIELDS: SettingsField[] = ["concurrent", "limit", "connections", "proxy"];

/** Which field a single-field patch from `fieldPatch` / `dirtyPatches` sets. */
export function fieldOf(patch: SettingsPatch): SettingsField {
  if (patch.maxConcurrent != null) return "concurrent";
  if (patch.globalLimit != null) return "limit";
  if (patch.proxy != null) return "proxy";
  return "connections";
}

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
