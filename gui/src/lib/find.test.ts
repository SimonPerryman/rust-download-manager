import { describe, expect, it } from "vitest";
import {
  findReducer,
  inOldFolder,
  initialFindState,
  initialTicks,
  reviewRows,
  searchingMany,
  searchingOne,
  sharedFolder,
  type FindState,
} from "./find";
import { snap } from "./testSnapshot";
import type { FindEvent, FindResult, FoundFile } from "./types";

const file = (path: string, reason: FoundFile["reason"] = "SameNameAndSize"): FoundFile => ({
  path,
  size: 10,
  modified: null,
  reason,
});
const result = (id: number, matches: FoundFile[] = [], truncated = false): FindResult => ({
  id,
  matches,
  searched_folders: ["/a", "/b"],
  truncated,
});
const finished = (search: number, cancelled = false): Extract<FindEvent, { kind: "finished" }> => ({
  kind: "finished",
  search,
  cancelled,
  searched_folders: ["/a"],
  truncated: false,
});
const begin = (ids: number[], many: boolean): FindState =>
  findReducer(initialFindState, { type: "begin", ids, many });

describe("findReducer", () => {
  it("adopts the first event's search id when events beat the start call's answer", () => {
    let s = begin([1], false);
    s = findReducer(s, {
      type: "event",
      event: { kind: "progress", search: 7, folders_done: 1, folders_total: 4, current: "/b" },
    });
    expect(s.active).toMatchObject({ search: 7, foldersDone: 1, foldersTotal: 4, current: "/b" });
    // The late answer changes nothing.
    expect(findReducer(s, { type: "attach", search: 7 })).toBe(s);
  });

  it("ignores events of a search it is not following", () => {
    let s = findReducer(begin([1], false), { type: "attach", search: 3 });
    const before = s;
    s = findReducer(s, {
      type: "event",
      event: { kind: "result", search: 2, result: result(1, [file("/x")]) },
    });
    expect(s).toBe(before);
  });

  it("keeps a result per download and drops the old one when a new search begins", () => {
    let s = begin([1, 2], true);
    s = findReducer(s, { type: "event", event: { kind: "result", search: 1, result: result(1) } });
    expect(Object.keys(s.results)).toEqual(["1"]);
    s = findReducer(s, { type: "synced", results: [], finished: finished(1) });
    s = findReducer(s, { type: "begin", ids: [1], many: false });
    expect(s.results[1]).toBeUndefined();
  });

  it("opens the review when a Find files search finishes, not when it is cancelled or one file", () => {
    const done = (ids: number[], many: boolean, cancelled = false) => {
      const s = begin(ids, many);
      return findReducer(s, {
        type: "synced",
        results: [result(1, [file("/a/x")])],
        finished: finished(1, cancelled),
      });
    };
    expect(done([1, 2], true).review).toEqual([1, 2]);
    expect(done([1, 2], true).active).toBeNull();
    expect(done([1, 2], true, true).review).toBeNull();
    expect(done([1], false).review).toBeNull();
    expect(done([1], false).results[1]).toBeDefined();
  });

  it("takes the engine's kept results as the truth after the end", () => {
    let s = begin([1, 2], true);
    // Result 2 never arrived as an event (a lagged window).
    s = findReducer(s, {
      type: "synced",
      results: [result(1), result(2, [file("/z")])],
      finished: finished(5),
    });
    expect(s.results[2].matches).toHaveLength(1);
  });

  it("clears a result on Skip and drops results of downloads no longer missing", () => {
    let s: FindState = {
      results: { 1: result(1), 2: result(2), 3: result(3) },
      active: null,
      review: [1, 2, 3],
    };
    s = findReducer(s, { type: "skip", id: 3 });
    expect(Object.keys(s.results)).toEqual(["1", "2"]);
    s = findReducer(s, { type: "prune", missing: new Set([2]) });
    expect(Object.keys(s.results)).toEqual(["2"]);
    expect(s.review).toEqual([2]);
    s = findReducer(s, { type: "prune", missing: new Set() });
    expect(s.review).toBeNull();
    // Nothing to prune: the same state back.
    expect(findReducer(s, { type: "prune", missing: new Set([9]) })).toBe(s);
  });

  it("tells which search is the bar's and which is a record's panel", () => {
    const one = begin([4], false);
    const many = begin([4, 5], true);
    expect(searchingOne(one, 4)).not.toBeNull();
    expect(searchingOne(one, 5)).toBeNull();
    expect(searchingOne(many, 4)).toBeNull();
    expect(searchingMany(many)?.ids).toEqual([4, 5]);
    expect(searchingMany(one)).toBeNull();
  });
});

describe("review rows", () => {
  const gone = (id: number, final_path: string) =>
    snap({ id, status: "completed", file_missing: true, final_path });
  const results = {
    1: result(1, [file("E:\\Archive\\a.iso")]),
    2: result(2, [file("E:\\Archive\\b-copy.zip", "RenamedUniqueSize")]),
    3: result(3),
  };
  const downloads = [
    gone(1, "D:\\Downloads\\a.iso"),
    gone(2, "D:\\Downloads\\b.zip"),
    gone(3, "D:\\Downloads\\c.msi"),
  ];

  it("starts exact matches ticked and renamed ones unticked", () => {
    const rows = reviewRows([1, 2, 3], downloads, results);
    expect([...initialTicks(rows)]).toEqual([1]);
    expect(rows[2].match).toBeUndefined();
  });

  it("leaves out downloads that are no longer missing", () => {
    const relinked = [{ ...downloads[0], file_missing: false }, downloads[1], downloads[2]];
    expect(reviewRows([1, 2, 3], relinked, results).map((r) => r.d.id)).toEqual([2, 3]);
  });

  it("names the folder only when every found file is in it", () => {
    const rows = reviewRows([1, 2, 3], downloads, results);
    expect(sharedFolder(rows)).toBe("E:\\Archive");
    const mixed = reviewRows([1, 2], downloads, {
      ...results,
      2: result(2, [file("C:\\Other\\b.zip")]),
    });
    expect(sharedFolder(mixed)).toBeNull();
  });
});

describe("inOldFolder", () => {
  it("picks the missing downloads that were in the folder, ignoring case and separators", () => {
    const ds = [
      snap({ id: 1, status: "completed", file_missing: true, final_path: "D:\\Downloads\\a.iso" }),
      snap({ id: 2, status: "completed", file_missing: true, final_path: "d:/downloads/b.zip" }),
      snap({ id: 3, status: "completed", file_missing: true, final_path: "D:\\Other\\c.msi" }),
      snap({ id: 4, status: "completed", file_missing: false, final_path: "D:\\Downloads\\d" }),
    ];
    expect(inOldFolder(ds, String.raw`D:\Downloads`)).toEqual([1, 2]);
  });
});
