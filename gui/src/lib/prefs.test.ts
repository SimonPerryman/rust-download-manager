/// <reference types="vite/client" />
import { describe, expect, it } from "vitest";
import indexHtml from "../../index.html?raw";
import {
  applyPrefs,
  DEFAULT_PREFS,
  LAST_DIR_KEY,
  LOG_DETAILED_KEY,
  loadLastDir,
  loadLogDetailed,
  loadPaneHeight,
  loadPrefs,
  loadUnopened,
  PANE_HEIGHT_KEY,
  parsePrefs,
  PREFS_KEY,
  resolveTheme,
  saveLastDir,
  saveLogDetailed,
  savePaneHeight,
  savePrefs,
  saveUnopened,
  UNOPENED_KEY,
  type Prefs,
} from "./prefs";

function memoryStorage(initial: Record<string, string> = {}) {
  const data = { ...initial };
  return {
    data,
    getItem: (k: string) => (k in data ? data[k] : null),
    setItem: (k: string, v: string) => {
      data[k] = v;
    },
  };
}

const throwing = {
  getItem: (): string | null => {
    throw new Error("SecurityError");
  },
  setItem: () => {
    throw new Error("QuotaExceededError");
  },
};

function fakeRoot() {
  const classes = new Set<string>();
  return {
    dataset: {} as Record<string, string | undefined>,
    classes,
    classList: {
      toggle(token: string, force?: boolean) {
        const on = force ?? !classes.has(token);
        if (on) classes.add(token);
        else classes.delete(token);
        return on;
      },
    },
  };
}

describe("parsePrefs", () => {
  it("defaults to compact rows and the system theme", () => {
    expect(DEFAULT_PREFS).toEqual({ theme: "system", density: "compact" });
  });

  it("returns defaults for missing, corrupt, or non-object values", () => {
    for (const raw of [null, "", "not json", "null", "42", '"dark"', "[]"]) {
      expect(parsePrefs(raw)).toEqual(DEFAULT_PREFS);
    }
  });

  it("falls back per field, keeping valid fields", () => {
    expect(parsePrefs(JSON.stringify({ theme: "neon", rows: "compact" }))).toEqual({
      theme: "system",
      density: "compact",
    });
    expect(parsePrefs(JSON.stringify({ theme: "dark", rows: 3 }))).toEqual({
      theme: "dark",
      density: "compact",
    });
    expect(parsePrefs(JSON.stringify({ rows: "comfortable" })).density).toBe("comfortable");
  });

  it("ignores the older density field, saved before rows could be switched", () => {
    expect(parsePrefs(JSON.stringify({ theme: "dark", density: "comfortable" }))).toEqual({
      theme: "dark",
      density: "compact",
    });
  });
});

describe("loadPrefs / savePrefs", () => {
  it("round-trips through storage under the namespaced key", () => {
    const storage = memoryStorage();
    const prefs: Prefs = { theme: "light", density: "compact" };
    expect(savePrefs(prefs, storage)).toBe(true);
    expect(Object.keys(storage.data)).toEqual([PREFS_KEY]);
    expect(loadPrefs(storage)).toEqual(prefs);
  });

  it("uses defaults when nothing is stored", () => {
    expect(loadPrefs(memoryStorage())).toEqual(DEFAULT_PREFS);
  });

  it("survives storage that throws or is unavailable", () => {
    expect(loadPrefs(throwing)).toEqual(DEFAULT_PREFS);
    expect(loadPrefs(undefined)).toEqual(DEFAULT_PREFS);
    expect(savePrefs(DEFAULT_PREFS, throwing)).toBe(false);
    expect(savePrefs(DEFAULT_PREFS, undefined)).toBe(false);
  });
});

describe("loadUnopened / saveUnopened", () => {
  it("round-trips the ids under their own key", () => {
    const storage = memoryStorage();
    expect(saveUnopened(new Set([3, 1]), storage)).toBe(true);
    expect(Object.keys(storage.data)).toEqual([UNOPENED_KEY]);
    expect(loadUnopened(storage)).toEqual(new Set([3, 1]));
  });

  it("reads garbage or nothing as no ids", () => {
    expect(loadUnopened(memoryStorage())).toEqual(new Set());
    expect(loadUnopened(memoryStorage({ [UNOPENED_KEY]: "{oops" }))).toEqual(new Set());
    expect(loadUnopened(memoryStorage({ [UNOPENED_KEY]: '[1,"x",2]' }))).toEqual(new Set([1, 2]));
  });

  it("survives storage that throws or is unavailable", () => {
    expect(loadUnopened(throwing)).toEqual(new Set());
    expect(loadUnopened(undefined)).toEqual(new Set());
    expect(saveUnopened(new Set([1]), throwing)).toBe(false);
    expect(saveUnopened(new Set([1]), undefined)).toBe(false);
  });
});

describe("loadPaneHeight / savePaneHeight", () => {
  it("round-trips a whole number of pixels under its own key", () => {
    const storage = memoryStorage();
    expect(savePaneHeight(301.6, storage)).toBe(true);
    expect(storage.data).toEqual({ [PANE_HEIGHT_KEY]: "302" });
    expect(loadPaneHeight(storage)).toBe(302);
  });

  it("reads garbage, nothing or a non-positive value as no height", () => {
    expect(loadPaneHeight(memoryStorage())).toBeNull();
    expect(loadPaneHeight(memoryStorage({ [PANE_HEIGHT_KEY]: "tall" }))).toBeNull();
    expect(loadPaneHeight(memoryStorage({ [PANE_HEIGHT_KEY]: "-5" }))).toBeNull();
  });

  it("survives storage that throws or is unavailable", () => {
    expect(loadPaneHeight(throwing)).toBeNull();
    expect(loadPaneHeight(undefined)).toBeNull();
    expect(savePaneHeight(200, throwing)).toBe(false);
    expect(savePaneHeight(200, undefined)).toBe(false);
  });
});

describe("resolveTheme / applyPrefs", () => {
  it("system follows the OS; explicit choices override it", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("stamps data attributes and toggles the dark class", () => {
    const root = fakeRoot();
    applyPrefs({ theme: "dark", density: "compact" }, root, false);
    expect(root.dataset).toEqual({ theme: "dark", density: "compact" });
    expect(root.classes.has("dark")).toBe(true);

    applyPrefs({ theme: "system", density: "comfortable" }, root, false);
    expect(root.dataset).toEqual({ theme: "system", density: "comfortable" });
    expect(root.classes.has("dark")).toBe(false);
  });
});

describe("loadLastDir / saveLastDir", () => {
  it("round-trips the Add dialog's last folder", () => {
    const storage = memoryStorage();
    expect(loadLastDir(storage)).toBeNull();
    expect(saveLastDir("/data/isos", storage)).toBe(true);
    expect(storage.data[LAST_DIR_KEY]).toBe("/data/isos");
    expect(loadLastDir(storage)).toBe("/data/isos");
  });

  it("treats a blank value or failing storage as none", () => {
    expect(loadLastDir(memoryStorage({ [LAST_DIR_KEY]: "  " }))).toBeNull();
    expect(loadLastDir(throwing)).toBeNull();
    expect(saveLastDir("/x", throwing)).toBe(false);
  });
});

describe("loadLogDetailed / saveLogDetailed", () => {
  it("starts concise and round-trips the Log tab's level", () => {
    const storage = memoryStorage();
    expect(loadLogDetailed(storage)).toBe(false);
    expect(saveLogDetailed(true, storage)).toBe(true);
    expect(storage.data[LOG_DETAILED_KEY]).toBe("true");
    expect(loadLogDetailed(storage)).toBe(true);
    expect(saveLogDetailed(false, storage)).toBe(true);
    expect(loadLogDetailed(storage)).toBe(false);
  });

  it("treats anything else, or failing storage, as concise", () => {
    expect(loadLogDetailed(memoryStorage({ [LOG_DETAILED_KEY]: "yes" }))).toBe(false);
    expect(loadLogDetailed(throwing)).toBe(false);
    expect(loadLogDetailed(undefined)).toBe(false);
    expect(saveLogDetailed(true, throwing)).toBe(false);
    expect(saveLogDetailed(true, undefined)).toBe(false);
  });
});

// index.html applies prefs before first paint with a hand-written mirror of
// parsePrefs + applyPrefs; run it against fakes to keep the two in step.
describe("index.html pre-paint script", () => {
  const script = /<script>([\s\S]*?)<\/script>/.exec(indexHtml)?.[1];

  function runInline(stored: string | null, systemDark: boolean, storageThrows = false) {
    const root = fakeRoot();
    const storage = storageThrows
      ? throwing
      : memoryStorage(stored == null ? {} : { [PREFS_KEY]: stored });
    const matchMedia = () => ({ matches: systemDark });
    new Function("document", "localStorage", "matchMedia", script!)(
      { documentElement: root },
      storage,
      matchMedia,
    );
    return root;
  }

  it("exists", () => {
    expect(script).toBeTruthy();
  });

  const cases: [string | null, boolean][] = [
    [null, true],
    [null, false],
    ["garbage", true],
    [JSON.stringify({ theme: "light", rows: "compact" }), true],
    [JSON.stringify({ theme: "dark", rows: "comfortable" }), false],
    [JSON.stringify({ theme: "dark", density: "comfortable" }), false],
    [JSON.stringify({ theme: "neon", rows: "tiny" }), true],
  ];
  it.each(cases)("matches applyPrefs for stored %s (system dark: %s)", (stored, systemDark) => {
    const expected = fakeRoot();
    applyPrefs(parsePrefs(stored), expected, systemDark);
    const actual = runInline(stored, systemDark);
    expect(actual.dataset).toEqual(expected.dataset);
    expect([...actual.classes]).toEqual([...expected.classes]);
  });

  it("falls back to defaults when storage throws", () => {
    const root = runInline(null, false, true);
    expect(root.dataset).toEqual({ theme: "system", density: "compact" });
  });
});
