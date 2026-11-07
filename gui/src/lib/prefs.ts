// Cosmetic GUI preferences (theme + row density) and the finish moment's
// "not opened yet" ids, kept in localStorage. Pure apart from the
// storage/root handles passed in, so it is unit-testable.
// The inline script in index.html mirrors `parsePrefs` + `applyPrefs` to apply
// them before first paint; keep the two in step.

import { parseIdList } from "./finish";

export const THEMES = ["system", "light", "dark"] as const;
export const DENSITIES = ["comfortable", "compact"] as const;

export type ThemePref = (typeof THEMES)[number];
export type Density = (typeof DENSITIES)[number];
export type Prefs = { theme: ThemePref; density: Density };

export const PREFS_KEY = "rdm.gui.prefs";
export const DEFAULT_PREFS: Prefs = { theme: "system", density: "comfortable" };

type ReadStorage = Pick<Storage, "getItem">;
type WriteStorage = Pick<Storage, "setItem">;

function oneOf<T extends string>(allowed: readonly T[], value: unknown, fallback: T): T {
  return allowed.includes(value as T) ? (value as T) : fallback;
}

/** Validates a stored JSON string field-by-field; anything unknown falls back to the default. */
export function parsePrefs(raw: string | null): Prefs {
  let obj: unknown = null;
  try {
    obj = raw == null ? null : JSON.parse(raw);
  } catch {
    // corrupt value: defaults below
  }
  const rec = obj !== null && typeof obj === "object" ? (obj as Record<string, unknown>) : {};
  return {
    theme: oneOf(THEMES, rec.theme, DEFAULT_PREFS.theme),
    density: oneOf(DENSITIES, rec.density, DEFAULT_PREFS.density),
  };
}

function defaultStorage(): Storage | undefined {
  // Merely touching `localStorage` can throw when storage is disabled.
  try {
    return globalThis.localStorage;
  } catch {
    return undefined;
  }
}

export function loadPrefs(storage: ReadStorage | undefined = defaultStorage()): Prefs {
  try {
    return parsePrefs(storage?.getItem(PREFS_KEY) ?? null);
  } catch {
    return { ...DEFAULT_PREFS };
  }
}

/** Persists prefs; returns false (and changes nothing) if storage is unavailable. */
export function savePrefs(
  prefs: Prefs,
  storage: WriteStorage | undefined = defaultStorage(),
): boolean {
  try {
    if (!storage) return false;
    storage.setItem(PREFS_KEY, JSON.stringify(prefs));
    return true;
  } catch {
    return false;
  }
}

/** Ids of downloads that finished while the app watched and haven't been opened yet. */
export const UNOPENED_KEY = "rdm.gui.unopened";

export function loadUnopened(storage: ReadStorage | undefined = defaultStorage()): Set<number> {
  try {
    return new Set(parseIdList(storage?.getItem(UNOPENED_KEY) ?? null));
  } catch {
    return new Set();
  }
}

/** Persists the unopened ids; returns false (and changes nothing) if storage is unavailable. */
export function saveUnopened(
  ids: Iterable<number>,
  storage: WriteStorage | undefined = defaultStorage(),
): boolean {
  try {
    if (!storage) return false;
    storage.setItem(UNOPENED_KEY, JSON.stringify([...ids]));
    return true;
  } catch {
    return false;
  }
}

export function resolveTheme(theme: ThemePref, systemDark: boolean): "light" | "dark" {
  if (theme === "system") return systemDark ? "dark" : "light";
  return theme;
}

type Root = {
  dataset: Record<string, string | undefined>;
  classList: { toggle(token: string, force?: boolean): boolean };
};

/**
 * Stamps prefs onto the document root: `data-theme` / `data-density` record the
 * choice (rows are a fixed 32 px until the density toggle returns, so nothing
 * reads `data-density` yet), and the `.dark` class that Tailwind's
 * `dark:` variants and the dark palette key off reflects the resolved theme.
 */
export function applyPrefs(prefs: Prefs, root: Root, systemDark: boolean): void {
  root.dataset.theme = prefs.theme;
  root.dataset.density = prefs.density;
  root.classList.toggle("dark", resolveTheme(prefs.theme, systemDark) === "dark");
}
