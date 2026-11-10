import { describe, expect, it } from "vitest";
import {
  addLabel,
  addLinks,
  knownUrls,
  linksToAdd,
  parseLinks,
  partialAddError,
  previewSummary,
  type Known,
} from "./batch";

const none = new Map<string, Known>();

describe("parseLinks", () => {
  it("reads one link per line, flagging lines that aren't links", () => {
    const entries = parseLinks(
      "https://a.example/one.iso\nnotes from the forum\n  http://b.example/dir/two.tar.gz  \n",
      none,
    );
    expect(entries).toEqual([
      {
        kind: "link",
        url: "https://a.example/one.iso",
        name: "one.iso",
        host: "a.example",
        dup: null,
      },
      { kind: "bad", line: "notes from the forum" },
      {
        kind: "link",
        url: "http://b.example/dir/two.tar.gz",
        name: "two.tar.gz",
        host: "b.example",
        dup: null,
      },
    ]);
  });

  it("skips blank and # comment lines and splits a line of several links", () => {
    const entries = parseLinks(
      "# from a uri-list\r\n\r\nhttps://a.example/x https://a.example/y",
      none,
    );
    expect(entries.map((e) => (e.kind === "link" ? e.url : e.line))).toEqual([
      "https://a.example/x",
      "https://a.example/y",
    ]);
  });

  it("keeps a line of prose with a link in it as one non-link", () => {
    expect(parseLinks("see https://a.example/x for details", none)).toEqual([
      { kind: "bad", line: "see https://a.example/x for details" },
    ]);
  });

  it("rejects other schemes", () => {
    expect(
      parseLinks("ftp://a.example/x\nfile:///tmp/x", none).every((e) => e.kind === "bad"),
    ).toBe(true);
  });

  it("names a link by its decoded last path part, else its host", () => {
    const [a, b] = parseLinks("https://a.example/my%20file.zip\nhttps://b.example/", none);
    expect(a).toMatchObject({ name: "my file.zip" });
    expect(b).toMatchObject({ name: "b.example" });
    const [bad] = parseLinks("https://a.example/%E0%A4%A", none);
    expect(bad).toMatchObject({ name: "%E0%A4%A" });
  });

  it("flags links already in the list and repeats within the paste", () => {
    const entries = parseLinks(
      "https://a.example/x\nhttps://a.example/y\nhttps://a.example/x",
      new Map([["https://a.example/y", "listed" as const]]),
    );
    expect(entries.map((e) => (e.kind === "link" ? e.dup : "bad"))).toEqual([
      null,
      "listed",
      "repeat",
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
    const entries = parseLinks("https://a.example/y", known);
    expect(entries).toMatchObject([{ kind: "link", dup: "archived" }]);
    expect(linksToAdd(entries)).toEqual(["https://a.example/y"]);
  });
});

describe("linksToAdd", () => {
  it("adds listed duplicates but not repeats or non-links", () => {
    const entries = parseLinks(
      "https://a.example/x\nnope\nhttps://a.example/x\nhttps://a.example/y",
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
    expect(previewSummary(parseLinks("https://a.example/x", none))).toEqual({
      found: "1 link found",
      skipped: null,
    });
    expect(
      previewSummary(parseLinks("https://a.example/x\nhttps://a.example/y\nnope", none)),
    ).toEqual({ found: "2 links found", skipped: "1 line isn't a link, skipped" });
    expect(
      previewSummary(
        parseLinks("a\nb\nhttps://a.example/x\nhttps://a.example/x\nhttps://a.example/x", none),
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
