import { describe, expect, it } from "vitest";
import {
  refusalMessage,
  addLabel,
  addLinks,
  entriesFrom,
  knownUrls,
  linksToAdd,
  partialAddError,
  previewSummary,
  type Known,
} from "./batch";
import type { ParsedList } from "./types";

const none = new Map<string, Known>();

/** What the engine's `preview_list` answers (see src/engine/batch.rs). */
const list = (links: [string, number?][], skipped: ParsedList["skipped"] = []): ParsedList => ({
  links: links.map(([url, count]) => ({ url, count: count ?? 1, pattern: null })),
  patterns: [],
  skipped,
});
const notLink = (line: string): ParsedList["skipped"][number] => ({
  line,
  reason: "not_a_link",
  count: null,
});

/** A range of `n` links `https://a.example/lessonNN.mp4`, as the engine reports it. */
const range = (n: number, extra: ParsedList["skipped"] = []): ParsedList => ({
  links: Array.from({ length: n }, (_, i) => ({
    url: `https://a.example/lesson${String(i + 1).padStart(2, "0")}.mp4`,
    count: 1,
    pattern: 0,
  })),
  patterns: [{ stem: "lesson", count: n }],
  skipped: extra,
});

describe("entriesFrom", () => {
  it("shows each link's name and host, then the skipped lines", () => {
    const entries = entriesFrom(
      list([["https://a.example/dir/file.zip"]], [notLink("nope")]),
      none,
    );
    expect(entries).toEqual([
      {
        kind: "link",
        url: "https://a.example/dir/file.zip",
        name: "file.zip",
        host: "a.example",
        count: 1,
        dup: null,
      },
      { kind: "bad", line: "nope", reason: "not_a_link" },
    ]);
  });

  it("names a link by its decoded last path part, else its host", () => {
    const [a, b, c] = entriesFrom(
      list([
        ["https://a.example/my%20file.zip"],
        ["https://b.example/"],
        ["https://a.example/%E0%A4%A"],
      ]),
      none,
    );
    expect(a).toMatchObject({ name: "my file.zip" });
    expect(b).toMatchObject({ name: "b.example" });
    expect(c).toMatchObject({ name: "%E0%A4%A" });
  });

  it("carries a repeat's count and flags links already in the list", () => {
    const entries = entriesFrom(
      list([["https://a.example/x", 2], ["https://a.example/y"]]),
      new Map([["https://a.example/y", "listed" as const]]),
    );
    expect(entries).toMatchObject([
      { kind: "link", count: 2, dup: null },
      { kind: "link", count: 1, dup: "listed" },
    ]);
  });
});

describe("knownUrls", () => {
  it("marks archived-only URLs apart from listed ones, listed winning", () => {
    const known = knownUrls([
      { url: "https://a.example/x", archived: false },
      { url: "https://a.example/y", archived: true },
      { url: "https://a.example/z", archived: true },
      { url: "https://a.example/z", archived: false },
    ]);
    expect([...known]).toEqual([
      ["https://a.example/x", "listed"],
      ["https://a.example/y", "archived"],
      ["https://a.example/z", "listed"],
    ]);
  });

  it("notes an archived match and still adds it, so it can be downloaded again", () => {
    const known = knownUrls([{ url: "https://a.example/y", archived: true }]);
    const entries = entriesFrom(list([["https://a.example/y"]]), known);
    expect(entries).toMatchObject([{ kind: "link", dup: "archived" }]);
    expect(linksToAdd(entries)).toEqual(["https://a.example/y"]);
  });
});

describe("linksToAdd", () => {
  it("gives each link once, in list order, never the skipped lines", () => {
    const entries = entriesFrom(
      list([["https://a.example/x", 3], ["https://a.example/y"]], [notLink("nope")]),
      new Map([["https://a.example/y", "listed" as const]]),
    );
    expect(linksToAdd(entries)).toEqual(["https://a.example/x", "https://a.example/y"]);
  });
});

describe("addLabel", () => {
  it("counts from two links up", () => {
    expect(addLabel(0)).toBe("Add download");
    expect(addLabel(1)).toBe("Add download");
    expect(addLabel(2)).toBe("Add 2 downloads");
  });
});

describe("previewSummary", () => {
  it("counts links and says what's skipped", () => {
    expect(previewSummary(entriesFrom(list([["https://a.example/x"]]), none))).toEqual({
      found: "1 link found",
      skipped: null,
    });
    expect(
      previewSummary(
        entriesFrom(
          list([["https://a.example/x"], ["https://a.example/y"]], [notLink("nope")]),
          none,
        ),
      ),
    ).toEqual({
      found: "2 links found",
      skipped: { label: "1 skipped", reasons: ["1 line isn't a link"] },
    });
    expect(
      previewSummary(
        entriesFrom(list([["https://a.example/x", 3]], [notLink("a"), notLink("b")]), none),
      ),
    ).toEqual({
      found: "1 link found",
      skipped: { label: "4 skipped", reasons: ["2 lines aren't links", "2 repeats"] },
    });
  });
});

describe("addLinks / partialAddError", () => {
  it("adds every link in order, carrying on past a rejected one", async () => {
    const seen: string[] = [];
    const outcome = await addLinks(["a", "bad", "c"], async (url) => {
      seen.push(url);
      if (url === "bad") throw new Error("not reachable");
    });
    expect(seen).toEqual(["a", "bad", "c"]);
    expect(outcome).toEqual({ added: 2, failed: [{ url: "bad", error: "Error: not reachable" }] });
    expect(partialAddError(outcome)).toBe(
      "Added 2. This link couldn't be added: Error: not reachable",
    );
  });

  it("says These links when several failed", async () => {
    const outcome = await addLinks(["a", "b", "c"], async (url) => {
      if (url !== "a") throw "nope";
    });
    expect(partialAddError(outcome)).toBe("Added 1. These links couldn't be added: nope");
  });
});

describe("a range's row", () => {
  it("gathers a range's links under one row named by its first and last file", () => {
    const list = range(12);
    list.links.push({ url: "https://a.example/slides.pdf", count: 2, pattern: null });
    const entries = entriesFrom(list, none);
    expect(entries).toHaveLength(2);
    expect(entries[0]).toMatchObject({
      kind: "pattern",
      label: "lesson01.mp4 to lesson12.mp4",
    });
    expect(entries[0].kind === "pattern" && entries[0].links).toHaveLength(12);
    expect(entries[1]).toMatchObject({ kind: "link", name: "slides.pdf", count: 2 });
  });

  it("counts a range's links toward the total, and adds each once", () => {
    const entries = entriesFrom(range(12), none);
    expect(linksToAdd(entries)).toHaveLength(12);
    expect(previewSummary(entries)).toEqual({ found: "12 links found", skipped: null });
    expect(addLabel(linksToAdd(entries).length)).toBe("Add 12 downloads");
  });

  it("names a one-link range by its file", () => {
    expect(entriesFrom(range(1), none)[0]).toMatchObject({
      kind: "pattern",
      label: "lesson01.mp4",
    });
  });

  it("keeps a range's links split when a later link repeats one of them", () => {
    const list = range(3);
    list.links[1].count = 2;
    expect(previewSummary(entriesFrom(list, none))).toEqual({
      found: "3 links found",
      skipped: { label: "1 skipped", reasons: ["1 repeat"] },
    });
  });
});

describe("refused ranges", () => {
  it("are their own entries, not links and not 'lines that aren't links'", () => {
    const entries = entriesFrom(
      list(
        [["https://a.example/ok"]],
        [
          { line: "https://a.example/p[1-9000000000].bin", reason: "range_too_big", count: 9e9 },
          notLink("nope"),
        ],
      ),
      none,
    );
    expect(entries.map((e) => e.kind)).toEqual(["link", "refused", "bad"]);
    expect(linksToAdd(entries)).toEqual(["https://a.example/ok"]);
    expect(previewSummary(entries).skipped?.reasons).toEqual(["1 line isn't a link"]);
  });

  it("say how big a too-big range is, with the cap", () => {
    const [e] = entriesFrom(
      list([], [{ line: "x", reason: "range_too_big", count: 9_000_000_000 }]),
      none,
    );
    if (e.kind !== "refused") throw new Error("expected a refused range");
    expect(refusalMessage(e)).toBe("That range makes 9,000,000,000 links. The most is 500.");
  });

  it("explain each other refusal", () => {
    const say = (reason: Extract<ParsedList["skipped"][number]["reason"], `range_${string}`>) => {
      const [e] = entriesFrom(list([], [{ line: "x", reason, count: null }]), none);
      return e.kind === "refused" ? refusalMessage(e) : "";
    };
    expect(say("range_reversed")).toMatch(/count up/);
    expect(say("range_width_mismatch")).toMatch(/same number of digits/);
    expect(say("range_in_host")).toMatch(/host/);
    expect(say("range_invalid")).toMatch(/numbers or both be single letters/);
  });
});
