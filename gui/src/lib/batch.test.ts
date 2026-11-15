import { describe, expect, it } from "vitest";
import {
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
  links: links.map(([url, count]) => ({ url, count: count ?? 1 })),
  skipped,
});
const notLink = (line: string): ParsedList["skipped"][number] => ({
  line,
  reason: "not_a_link",
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
    ).toEqual({ found: "2 links found", skipped: "1 line isn't a link, skipped" });
    expect(
      previewSummary(
        entriesFrom(list([["https://a.example/x", 3]], [notLink("a"), notLink("b")]), none),
      ),
    ).toEqual({ found: "1 link found", skipped: "2 lines aren't links and 2 repeats, skipped" });
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
