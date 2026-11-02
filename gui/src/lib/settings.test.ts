import { describe, expect, it } from "vitest";
import {
  dirtyPatches,
  draftsFrom,
  fieldPatch,
  formatRate,
  mergeApplied,
  parseCount,
} from "./settings";
import type { Settings } from "./types";

describe("formatRate", () => {
  it("is blank for unlimited", () => {
    expect(formatRate(null)).toBe("");
    expect(formatRate(0)).toBe("");
  });

  it("uses the largest exact binary unit", () => {
    expect(formatRate(2 * 1024 * 1024)).toBe("2M");
    expect(formatRate(500 * 1024)).toBe("500K");
    expect(formatRate(1536 * 1024)).toBe("1536K");
    expect(formatRate(3 * 1024 ** 3)).toBe("3G");
  });

  it("falls back to plain bytes", () => {
    expect(formatRate(1000)).toBe("1000");
    expect(formatRate(1)).toBe("1");
  });
});

describe("parseCount", () => {
  it("parses whole numbers, trimming whitespace", () => {
    expect(parseCount(" 4 ", 1, 16)).toBe(4);
  });

  it("clamps into range", () => {
    expect(parseCount("0", 1, 16)).toBe(1);
    expect(parseCount("99", 1, 16)).toBe(16);
  });

  it("rejects anything that isn't a whole number", () => {
    expect(parseCount("", 1, 16)).toBeNull();
    expect(parseCount("2.5", 1, 16)).toBeNull();
    expect(parseCount("-1", 1, 16)).toBeNull();
    expect(parseCount("abc", 1, 16)).toBeNull();
  });
});

const saved: Settings = { max_concurrent: 3, global_limit: null, max_connections: 4 };

describe("fieldPatch", () => {
  it("sends only the edited field", () => {
    expect(fieldPatch("concurrent", "1")).toEqual({ maxConcurrent: 1 });
    expect(fieldPatch("limit", " 2M ")).toEqual({ globalLimit: "2M" });
    expect(fieldPatch("connections", "99")).toEqual({ maxConnections: 16 });
  });

  it("sends a blank limit as unlimited", () => {
    expect(fieldPatch("limit", "")).toEqual({ globalLimit: "" });
  });

  it("rejects a count that isn't a whole number", () => {
    expect(fieldPatch("concurrent", "two")).toBeNull();
  });
});

describe("dirtyPatches", () => {
  it("is empty when nothing changed", () => {
    expect(dirtyPatches(draftsFrom(saved), saved)).toEqual([]);
  });

  it("sends every dirty field as its own update", () => {
    const drafts = { concurrent: "1", limit: "2M", connections: "8" };
    expect(dirtyPatches(drafts, saved)).toEqual([
      { maxConcurrent: 1 },
      { globalLimit: "2M" },
      { maxConnections: 8 },
    ]);
  });

  it("keeps a rate the engine will reject apart from valid edits", () => {
    // "0" passes any local shape check but the engine rejects it; bundled
    // with the cap edit, that rejection would drop the cap too.
    const drafts = { ...draftsFrom(saved), concurrent: "5", limit: "0" };
    expect(dirtyPatches(drafts, saved)).toEqual([{ maxConcurrent: 5 }, { globalLimit: "0" }]);
  });

  it("skips a count that isn't a whole number", () => {
    const drafts = { concurrent: "abc", limit: "", connections: "8" };
    expect(dirtyPatches(drafts, saved)).toEqual([{ maxConnections: 8 }]);
  });

  it("flushes clearing the limit back to unlimited", () => {
    const limited = { ...saved, global_limit: 2 * 1024 * 1024 };
    const drafts = { ...draftsFrom(limited), limit: "" };
    expect(dirtyPatches(drafts, limited)).toEqual([{ globalLimit: "" }]);
  });
});

describe("mergeApplied", () => {
  it("takes only the committed field from the response", () => {
    // The newer response (limit) landed first; the older one (cap) must not
    // roll the limit back.
    const afterLimit = { ...saved, global_limit: 2048 };
    const staleCapResponse = { ...saved, max_concurrent: 1 };
    expect(mergeApplied(afterLimit, staleCapResponse, ["concurrent"])).toEqual({
      max_concurrent: 1,
      global_limit: 2048,
      max_connections: 4,
    });
  });

  it("adopts the whole response when nothing was loaded", () => {
    expect(mergeApplied(null, saved, ["limit"])).toEqual(saved);
  });
});
