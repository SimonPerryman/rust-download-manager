// Cosmetic GUI preferences (theme + row density), the finish moment's
// "not opened yet" ids, the details pane's height and the Add dialog's last
// folder, kept in localStorage. Pure apart from the storage/root handles
// passed in, so it is unit-testable.
// The inline script in index.html mirrors `parsePrefs` + `applyPrefs` to apply
// them before first paint; keep the two in step.

import { parseIdList } from "./finish";

export const THEMES = ["system", "light", "dark"] as const;
export const DENSITIES = ["comfortable", "compact"] as const;

export type ThemePref = (typeof THEMES)[number];
export type Density = (typeof DENSITIES)[number];
export type Prefs = { theme: ThemePref; density: Density };

export const PREFS_KEY = "rdm.gui.prefs";
export const DEFAULT_PREFS: Prefs = { theme: "system", density: "compact" };

type ReadStorage = Pick<Storage, "getItem">;
type WriteStorage = Pick<Storage, "setItem">;

function oneOf<T extends string>(allowed: readonly T[], value: unknown, fallback: T): T {
  return allowed.includes(value as T) ? (value as T) : fallback;
}

/**
 * Validates a stored JSON string field-by-field; anything unknown falls back to
 * the default. Density is stored as `rows`: an older `density` field was saved
 * whenever the theme changed, before rows could be switched, so it isn't a choice.
 */
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
    density: oneOf(DENSITIES, rec.rows, DEFAULT_PREFS.density),
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
    storage.setItem(PREFS_KEY, JSON.stringify({ theme: prefs.theme, rows: prefs.density }));
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

/** The folder the last Add saved to, so the next one starts there. */
export const LAST_DIR_KEY = "rdm.gui.lastDir";

/** The remembered folder, or null when none is stored (or storage is unavailable). */
export function loadLastDir(storage: ReadStorage | undefined = defaultStorage()): string | null {
  try {
    const dir = storage?.getItem(LAST_DIR_KEY) ?? null;
    return dir && dir.trim() ? dir : null;
  } catch {
    return null;
  }
}

/** Remembers `dir`; returns false (and changes nothing) if storage is unavailable. */
export function saveLastDir(
  dir: string,
  storage: WriteStorage | undefined = defaultStorage(),
): boolean {
  try {
    if (!storage) return false;
    storage.setItem(LAST_DIR_KEY, dir);
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
 * Stamps prefs onto the document root: `data-theme` records the choice,
 * `data-density` sets the row height (index.css), and the `.dark` class that
 * Tailwind's `dark:` variants and the dark palette key off reflects the
 * resolved theme.
 */
export function applyPrefs(prefs: Prefs, root: Root, systemDark: boolean): void {
  root.dataset.theme = prefs.theme;
  root.dataset.density = prefs.density;
  root.classList.toggle("dark", resolveTheme(prefs.theme, systemDark) === "dark");
}

/** The details pane's height in px, as the user last dragged it. */
export const PANE_HEIGHT_KEY = "rdm.gui.paneHeight";

export function loadPaneHeight(storage: ReadStorage | undefined = defaultStorage()): number | null {
  try {
    const n = Number(storage?.getItem(PANE_HEIGHT_KEY));
    return Number.isFinite(n) && n > 0 ? n : null;
  } catch {
    return null;
  }
}

/** Persists the pane height; returns false (and changes nothing) if storage is unavailable. */
export function savePaneHeight(
  px: number,
  storage: WriteStorage | undefined = defaultStorage(),
): boolean {
  try {
    if (!storage) return false;
    storage.setItem(PANE_HEIGHT_KEY, String(Math.round(px)));
    return true;
  } catch {
    return false;
  }
}
