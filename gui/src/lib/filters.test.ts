import { describe, expect, it } from "vitest";
import {
  archivedCount,
  filterKeyOf,
  isArchivedView,
  matchesFilter,
  matchesQuery,
  sameFilter,
  statusCounts,
  STATUS_FILTERS,
  type StatusFilter,
  type ViewFilter,
} from "./filters";
import { deriveFolders } from "./folders";
import { tagCounts } from "./tags";
import { snap } from "./testSnapshot";
import type { Status } from "./types";

const STATUSES: Status[] = ["queued", "downloading", "paused", "completed", "failed", "cancelled"];

describe("filterKeyOf", () => {
  it("maps each engine status to exactly one filter key", () => {
    expect(STATUSES.map(filterKeyOf)).toEqual([
      "waiting",
      "downloading",
      "paused",
      "done",
      "failed",
      "stopped",
    ]);
  });

  it("covers every status filter but All, once each", () => {
    // Missing is an overlay on Completed, not a status of its own.
    const keys = STATUS_FILTERS.map((f) => f.key).filter((k) => k !== "all" && k !== "missing");
    expect(new Set(STATUSES.map(filterKeyOf))).toEqual(new Set(keys));
    expect(STATUS_FILTERS[0]).toEqual({ key: "all", label: "All" });
  });
});

describe("matchesFilter", () => {
  it("All matches every status", () => {
    for (const status of STATUSES) {
      expect(matchesFilter(snap({ status }), { kind: "status", status: "all" })).toBe(true);
    }
  });

  it("a status filter matches only its statuses", () => {
    const d = snap({ status: "cancelled" });
    expect(matchesFilter(d, { kind: "status", status: "stopped" })).toBe(true);
    expect(matchesFilter(d, { kind: "status", status: "done" })).toBe(false);
  });

  it("a folder filter matches the target's parent dir", () => {
    const d = snap({ target: "C:\\Users\\a\\Downloads\\x.iso" });
    expect(matchesFilter(d, { kind: "folder", dir: "C:\\Users\\a\\Downloads" })).toBe(true);
    expect(matchesFilter(d, { kind: "folder", dir: "C:\\Users\\a" })).toBe(false);
  });
});

describe("matchesQuery", () => {
  const d = snap({ filename: "Ubuntu-24.04.iso", url: "https://Releases.Ubuntu.com/x" });

  it("hits the filename or the host, case-insensitively", () => {
    expect(matchesQuery(d, "ubuntu-24")).toBe(true);
    expect(matchesQuery(d, "RELEASES.ubuntu")).toBe(true);
    expect(matchesQuery(d, "fedora")).toBe(false);
  });

  it("a blank query matches everything, and the query is trimmed", () => {
    expect(matchesQuery(d, "")).toBe(true);
    expect(matchesQuery(d, "   ")).toBe(true);
    expect(matchesQuery(d, "  iso ")).toBe(true);
  });
});

describe("statusCounts", () => {
  it("is all zeros for no downloads", () => {
    const counts = statusCounts([]);
    for (const { key } of STATUS_FILTERS) expect(counts[key]).toBe(0);
  });

  it("counts each status, and the per-status counts sum to All", () => {
    const ds = [
      ...STATUSES.map((status, i) => snap({ id: i + 1, status })),
      snap({ id: 10, status: "completed" }),
    ];
    const counts = statusCounts(ds);
    expect(counts.all).toBe(7);
    expect(counts.done).toBe(2);
    expect(counts.waiting).toBe(1);
    const keys: StatusFilter[] = STATUS_FILTERS.map((f) => f.key).filter(
      (k) => k !== "all" && k !== "missing",
    );
    expect(keys.reduce((sum, k) => sum + counts[k], 0)).toBe(counts.all);
  });
});

describe("the Missing filter", () => {
  const missingFilter: ViewFilter = { kind: "status", status: "missing" };
  const gone = snap({ id: 1, status: "completed", file_missing: true });
  const fine = snap({ id: 2, status: "completed" });

  it("sits between Failed and Stopped", () => {
    const keys = STATUS_FILTERS.map((f) => f.key);
    expect(keys.indexOf("missing")).toBe(keys.indexOf("failed") + 1);
    expect(keys.indexOf("stopped")).toBe(keys.indexOf("missing") + 1);
  });

  it("shows a missing row under both Completed and Missing", () => {
    expect(matchesFilter(gone, missingFilter)).toBe(true);
    expect(matchesFilter(gone, { kind: "status", status: "done" })).toBe(true);
    expect(matchesFilter(gone, { kind: "status", status: "all" })).toBe(true);
    expect(matchesFilter(fine, missingFilter)).toBe(false);
  });

  it("ignores the flag on a download that isn't finished", () => {
    expect(matchesFilter(snap({ status: "downloading", file_missing: true }), missingFilter)).toBe(
      false,
    );
  });

  it("counts a missing row under Completed and Missing, once under All", () => {
    const counts = statusCounts([gone, fine]);
    expect(counts.all).toBe(2);
    expect(counts.done).toBe(2);
    expect(counts.missing).toBe(1);
  });

  it("leaves archived rows out of the count", () => {
    expect(statusCounts([{ ...gone, archived: true }]).missing).toBe(0);
  });
});

describe("sameFilter", () => {
  it("compares kind and value", () => {
    expect(sameFilter({ kind: "status", status: "all" }, { kind: "status", status: "all" })).toBe(
      true,
    );
    expect(sameFilter({ kind: "status", status: "all" }, { kind: "status", status: "done" })).toBe(
      false,
    );
    expect(sameFilter({ kind: "folder", dir: "/a" }, { kind: "folder", dir: "/a" })).toBe(true);
    expect(sameFilter({ kind: "folder", dir: "/a" }, { kind: "folder", dir: "/b" })).toBe(false);
    expect(sameFilter({ kind: "folder", dir: "all" }, { kind: "status", status: "all" })).toBe(
      false,
    );
  });
});

describe("the archive and tags", () => {
  // A fixture mixing archived, tagged and plain downloads across two folders.
  const ds = [
    snap({ id: 1, status: "completed", tags: ["work"], target: "/a/1.iso" }),
    snap({ id: 2, status: "completed", tags: ["Work", "iso"], archived: true, target: "/a/2.iso" }),
    snap({ id: 3, status: "cancelled", archived: true, target: "/b/3.iso" }),
    snap({ id: 4, status: "failed", tags: ["WORK"], target: "/b/4.iso" }),
    snap({ id: 5, status: "downloading", target: "/a/5.iso" }),
    snap({ id: 6, status: "failed", tags: ["old"], archived: true, target: "/a/6.iso" }),
  ];
  const ids = (f: ViewFilter) => ds.filter((d) => matchesFilter(d, f)).map((d) => d.id);
  const live = ds.filter((d) => !d.archived);

  it("leaves archived rows out of All, every status filter and every folder filter", () => {
    for (const { key } of STATUS_FILTERS) {
      expect(ids({ kind: "status", status: key }).every((id) => ![2, 3, 6].includes(id))).toBe(
        true,
      );
    }
    expect(ids({ kind: "status", status: "all" })).toEqual([1, 4, 5]);
    expect(ids({ kind: "folder", dir: "/a" })).toEqual([1, 5]);
    expect(ids({ kind: "folder", dir: "/b" })).toEqual([4]);
  });

  it("shows exactly the archived rows under Archived", () => {
    expect(ids({ kind: "archived" })).toEqual([2, 3, 6]);
    expect(isArchivedView({ kind: "archived" })).toBe(true);
    expect(isArchivedView({ kind: "tag", tag: "x", archived: true })).toBe(true);
    expect(isArchivedView({ kind: "tag", tag: "x", archived: false })).toBe(false);
  });

  it("counts what each filter shows: statuses, folders, Archived and tags", () => {
    const counts = statusCounts(ds);
    for (const { key } of STATUS_FILTERS) {
      expect(counts[key]).toBe(ids({ kind: "status", status: key }).length);
    }
    for (const f of deriveFolders(live)) {
      expect(f.count).toBe(ids({ kind: "folder", dir: f.dir }).length);
    }
    expect(archivedCount(ds)).toBe(ids({ kind: "archived" }).length);
    for (const archived of [false, true]) {
      for (const t of tagCounts(ds.filter((d) => d.archived === archived))) {
        expect(t.count).toBe(ids({ kind: "tag", tag: t.tag, archived }).length);
      }
    }
  });

  it("lists tags from non-archived rows only, once each ignoring case; Archived lists its own", () => {
    expect(tagCounts(live)).toEqual([{ tag: "work", count: 2 }]);
    expect(tagCounts(ds.filter((d) => d.archived))).toEqual([
      { tag: "iso", count: 1 },
      { tag: "old", count: 1 },
      { tag: "Work", count: 1 },
    ]);
    expect(ids({ kind: "tag", tag: "WORK", archived: false })).toEqual([1, 4]);
    expect(ids({ kind: "tag", tag: "work", archived: true })).toEqual([2]);
  });

  it("tells tag views apart by scope, not by case", () => {
    const a: ViewFilter = { kind: "tag", tag: "work", archived: false };
    expect(sameFilter(a, { kind: "tag", tag: "Work", archived: false })).toBe(true);
    expect(sameFilter(a, { kind: "tag", tag: "work", archived: true })).toBe(false);
    expect(sameFilter({ kind: "archived" }, { kind: "archived" })).toBe(true);
  });
});
