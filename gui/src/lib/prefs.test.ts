/// <reference types="vite/client" />
import { describe, expect, it } from "vitest";
import indexHtml from "../../index.html?raw";
import {
  applyPrefs,
  DEFAULT_PREFS,
  loadPrefs,
  parsePrefs,
  PREFS_KEY,
  resolveTheme,
  savePrefs,
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
  it("returns defaults for missing, corrupt, or non-object values", () => {
    for (const raw of [null, "", "not json", "null", "42", '"dark"', "[]"]) {
      expect(parsePrefs(raw)).toEqual(DEFAULT_PREFS);
    }
  });

  it("falls back per field, keeping valid fields", () => {
    expect(parsePrefs(JSON.stringify({ theme: "neon", density: "compact" }))).toEqual({
      theme: "system",
      density: "compact",
    });
    expect(parsePrefs(JSON.stringify({ theme: "dark", density: 3 }))).toEqual({
      theme: "dark",
      density: "comfortable",
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
    [JSON.stringify({ theme: "light", density: "compact" }), true],
    [JSON.stringify({ theme: "dark", density: "comfortable" }), false],
    [JSON.stringify({ theme: "neon", density: "tiny" }), true],
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
    expect(root.dataset).toEqual({ theme: "system", density: "comfortable" });
  });
});
