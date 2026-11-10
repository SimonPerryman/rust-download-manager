import { describe, expect, it } from "vitest";
import { hasTag, MAX_TAG_LEN, MAX_TAGS, parseTags, tagsError, unionTags } from "./tags";
import { snap } from "./testSnapshot";

describe("parseTags", () => {
  it("splits on commas, trims, drops blanks and repeats ignoring case", () => {
    expect(parseTags(" work, iso ,, Work,ISO , course-2")).toEqual(["work", "iso", "course-2"]);
    expect(parseTags("  ")).toEqual([]);
  });
});

describe("tagsError", () => {
  it("mirrors the engine's length and count rules", () => {
    expect(tagsError(["x".repeat(MAX_TAG_LEN)])).toBeNull();
    expect(tagsError(["é".repeat(MAX_TAG_LEN)])).toBeNull();
    expect(tagsError(["x".repeat(MAX_TAG_LEN + 1)])).toMatch(/too long/);
    const full = Array.from({ length: MAX_TAGS }, (_, i) => `t${i}`);
    expect(tagsError(["T0"], full)).toBeNull();
    expect(tagsError(["new"], full)).toMatch(/at most 16 tags/);
  });
});

describe("hasTag and unionTags", () => {
  it("match ignoring case and list each tag once", () => {
    const a = snap({ id: 1, tags: ["work", "iso"] });
    const b = snap({ id: 2, tags: ["ISO", "more"] });
    expect(hasTag(a, "WORK")).toBe(true);
    expect(hasTag(b, "work")).toBe(false);
    expect(unionTags([a, b])).toEqual(["work", "iso", "more"]);
  });
});
