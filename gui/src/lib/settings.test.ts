import { describe, expect, it } from "vitest";
import {
  dirtyPatches,
  draftsFrom,
  fieldOf,
  fieldPatch,
  isDirty,
  mergeApplied,
  parseCount,
} from "./settings";
import { NO_LIMIT, ZERO_RATE } from "./rate";
import type { Settings } from "./types";

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

const saved: Settings = {
  max_concurrent: 3,
  global_limit: null,
  max_connections: 4,
  proxy: null,
};

const MIB = 1024 * 1024;
const drafts = draftsFrom(saved);

describe("fieldPatch", () => {
  it("sends only the edited field", () => {
    expect(fieldPatch("concurrent", { ...drafts, concurrent: "1" })).toEqual({
      patch: { maxConcurrent: 1 },
    });
    expect(fieldPatch("limit", { ...drafts, limit: { amount: "2", unit: "M" } })).toEqual({
      patch: { globalLimit: "2M" },
    });
    expect(fieldPatch("connections", { ...drafts, connections: "99" })).toEqual({
      patch: { maxConnections: 16 },
    });
  });

  it("sends the proxy trimmed, leaving its validation to the engine", () => {
    expect(fieldPatch("proxy", { ...drafts, proxy: " socks5h://host:1080 " })).toEqual({
      patch: { proxy: "socks5h://host:1080" },
    });
    expect(fieldPatch("proxy", { ...drafts, proxy: "not a url" })).toEqual({
      patch: { proxy: "not a url" },
    });
  });

  it("sends an empty limit as unlimited", () => {
    expect(fieldPatch("limit", { ...drafts, limit: NO_LIMIT })).toEqual({
      patch: { globalLimit: "" },
    });
  });

  it("explains a draft it can't send", () => {
    expect(fieldPatch("concurrent", { ...drafts, concurrent: "two" })).toEqual({
      error: "Enter a whole number.",
    });
    expect(fieldPatch("limit", { ...drafts, limit: { amount: "0", unit: "K" } })).toEqual({
      error: ZERO_RATE,
    });
  });
});

describe("isDirty", () => {
  it("ignores the unit of an empty limit", () => {
    expect(isDirty("limit", { ...drafts, limit: { amount: "", unit: "M" } }, saved)).toBe(false);
  });

  it("sees a changed unit on a set limit", () => {
    const limited = { ...saved, global_limit: 2 * MIB };
    const edited = { ...draftsFrom(limited), limit: { amount: "2", unit: "K" as const } };
    expect(isDirty("limit", edited, limited)).toBe(true);
  });
});

describe("isDirty for the proxy", () => {
  const withProxy = { ...saved, proxy: "socks5h://alice:••••••@host:1080" };

  it("shows the saved, masked proxy and treats it as unchanged", () => {
    expect(draftsFrom(withProxy).proxy).toBe(withProxy.proxy);
    expect(isDirty("proxy", draftsFrom(withProxy), withProxy)).toBe(false);
    expect(
      isDirty("proxy", { ...draftsFrom(withProxy), proxy: ` ${withProxy.proxy} ` }, withProxy),
    ).toBe(false);
  });

  it("sees an edit, and clearing it", () => {
    expect(isDirty("proxy", { ...drafts, proxy: "http://host:3128" }, saved)).toBe(true);
    expect(isDirty("proxy", { ...draftsFrom(withProxy), proxy: "" }, withProxy)).toBe(true);
  });
});

describe("fieldOf", () => {
  it("names the field each single-field patch sets", () => {
    expect(fieldOf({ maxConcurrent: 2 })).toBe("concurrent");
    expect(fieldOf({ globalLimit: "" })).toBe("limit");
    expect(fieldOf({ maxConnections: 4 })).toBe("connections");
    expect(fieldOf({ proxy: "" })).toBe("proxy");
  });
});

describe("dirtyPatches", () => {
  it("is empty when nothing changed", () => {
    expect(dirtyPatches(drafts, saved)).toEqual([]);
  });

  it("sends every dirty field as its own update", () => {
    const edited = {
      concurrent: "1",
      limit: { amount: "2", unit: "M" as const },
      connections: "8",
      proxy: "http://host:3128",
    };
    expect(dirtyPatches(edited, saved)).toEqual([
      { maxConcurrent: 1 },
      { globalLimit: "2M" },
      { maxConnections: 8 },
      { proxy: "http://host:3128" },
    ]);
  });

  it("keeps a rate the engine will reject apart from valid edits", () => {
    // 0.0001 KB/s passes the local zero check but rounds to 0 bytes/s, which
    // the engine rejects; bundled with the cap edit, that would drop the cap too.
    const edited = { ...drafts, concurrent: "5", limit: { amount: "0.0001", unit: "K" as const } };
    expect(dirtyPatches(edited, saved)).toEqual([{ maxConcurrent: 5 }, { globalLimit: "0.0001K" }]);
  });

  it("skips drafts that can't be sent", () => {
    const edited = {
      concurrent: "abc",
      limit: { amount: "0", unit: "K" as const },
      connections: "8",
      proxy: "",
    };
    expect(dirtyPatches(edited, saved)).toEqual([{ maxConnections: 8 }]);
  });

  it("flushes clearing the limit back to unlimited", () => {
    const limited = { ...saved, global_limit: 2 * MIB };
    const edited = { ...draftsFrom(limited), limit: NO_LIMIT };
    expect(dirtyPatches(edited, limited)).toEqual([{ globalLimit: "" }]);
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
      proxy: null,
    });
  });

  it("adopts the whole response when nothing was loaded", () => {
    expect(mergeApplied(null, saved, ["limit"])).toEqual(saved);
  });
});
