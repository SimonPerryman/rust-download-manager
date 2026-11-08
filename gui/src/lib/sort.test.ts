import { describe, expect, it } from "vitest";
import {
  arrangeRows,
  compareBy,
  DEFAULT_SORT,
  isVolatile,
  nextSort,
  rankOf,
  type SortKey,
  type SortState,
} from "./sort";
import { snap as queuedSnap } from "./testSnapshot";
import type { DownloadSnapshot } from "./types";

// Paused unless a test says otherwise: queued rows fill their slots in queue order.
const snap = (o: Partial<DownloadSnapshot> = {}) => queuedSnap({ status: "paused", ...o });
const ids = (ds: DownloadSnapshot[]) => ds.map((d) => d.id);
const arrange = (ds: DownloadSnapshot[], sort: SortState, pinFailed = false) =>
  ids(arrangeRows(ds, { sort, frozen: null, pinFailed }));

const dl = (id: number, o: Partial<DownloadSnapshot> = {}) =>
  snap({ id, status: "downloading", ...o });

// For each key: rows 1..3 whose value ascends with the id, plus (except name, which is
// always known) row 4 whose value is unknown.
const CASES: Record<SortKey, DownloadSnapshot[]> = {
  name: [
    snap({ id: 1, filename: "file2.iso" }),
    snap({ id: 2, filename: "File10.iso" }),
    snap({ id: 3, filename: "zeta" }),
  ],
  size: [
    snap({ id: 1, total: 10 }),
    snap({ id: 2, total: 20 }),
    snap({ id: 3, total: 30 }),
    snap({ id: 4 }),
  ],
  progress: [
    snap({ id: 1, status: "paused", downloaded: 1, total: 10 }),
    dl(2, { downloaded: 5, total: 10 }),
    snap({ id: 3, status: "completed", downloaded: 10, total: 10 }),
    dl(4, { downloaded: 5 }),
  ],
  speed: [
    dl(1, { bytes_per_sec: 1 }),
    dl(2, { bytes_per_sec: 2 }),
    dl(3, { bytes_per_sec: 3 }),
    snap({ id: 4, status: "paused", bytes_per_sec: 99 }),
  ],
  left: [dl(1, { eta_secs: 1 }), dl(2, { eta_secs: 2 }), dl(3, { eta_secs: 3 }), dl(4)],
  connections: [
    dl(1, { connections: 1 }),
    dl(2, { connections: 2 }),
    dl(3, { connections: 3 }),
    snap({ id: 4, status: "completed", connections: 9 }),
  ],
  added: [
    snap({ id: 1, added_at: 100 }),
    snap({ id: 2, added_at: 200 }),
    snap({ id: 3, added_at: 300 }),
    snap({ id: 4 }),
  ],
};

describe("sorting by each key", () => {
  for (const [key, rows] of Object.entries(CASES) as [SortKey, DownloadSnapshot[]][]) {
    it(`${key}: ascending and descending, unknown values last`, () => {
      const shuffled = [...rows].reverse();
      const unknown = rows.slice(3).map((d) => d.id);
      expect(arrange(shuffled, { key, dir: "asc" })).toEqual([1, 2, 3, ...unknown]);
      expect(arrange(shuffled, { key, dir: "desc" })).toEqual([3, 2, 1, ...unknown]);
      expect(ids([...shuffled].sort(compareBy(key)))).toEqual([1, 2, 3, ...unknown]);
    });
  }

  it("ties break by id descending in both directions", () => {
    const ds = [snap({ id: 1, total: 5 }), snap({ id: 3, total: 5 }), snap({ id: 2, total: 5 })];
    expect(arrange(ds, { key: "size", dir: "asc" })).toEqual([3, 2, 1]);
    expect(arrange(ds, { key: "size", dir: "desc" })).toEqual([3, 2, 1]);
  });

  it("nulls stay last in both directions, newest first among them", () => {
    const ds = [snap({ id: 1 }), snap({ id: 2, total: 1 }), snap({ id: 3 })];
    expect(arrange(ds, { key: "size", dir: "asc" })).toEqual([2, 3, 1]);
    expect(arrange(ds, { key: "size", dir: "desc" })).toEqual([2, 3, 1]);
  });

  it("orders by size from the total, so it holds while `downloaded` grows", () => {
    const before = [dl(1, { total: 30, downloaded: 1 }), dl(2, { total: 20, downloaded: 19 })];
    const after = [dl(1, { total: 30, downloaded: 29 }), dl(2, { total: 20, downloaded: 20 })];
    expect(arrange(before, { key: "size", dir: "desc" })).toEqual([1, 2]);
    expect(arrange(after, { key: "size", dir: "desc" })).toEqual([1, 2]);
  });
});

describe("nextSort / DEFAULT_SORT / isVolatile", () => {
  it("defaults to newest added first", () => {
    expect(DEFAULT_SORT).toEqual({ key: "added", dir: "desc" });
  });

  it("toggles the same key; a new key starts name asc, others desc", () => {
    expect(nextSort({ key: "size", dir: "desc" }, "size")).toEqual({ key: "size", dir: "asc" });
    expect(nextSort({ key: "size", dir: "asc" }, "size")).toEqual({ key: "size", dir: "desc" });
    expect(nextSort(DEFAULT_SORT, "name")).toEqual({ key: "name", dir: "asc" });
    expect(nextSort(DEFAULT_SORT, "speed")).toEqual({ key: "speed", dir: "desc" });
  });

  it("flags the keys whose values move every tick", () => {
    const volatile = (Object.keys(CASES) as SortKey[]).filter(isVolatile);
    expect(volatile).toEqual(["progress", "speed", "left", "connections"]);
  });
});

describe("frozen order", () => {
  const sort: SortState = { key: "speed", dir: "desc" };
  const start = [
    dl(1, { bytes_per_sec: 10 }),
    dl(2, { bytes_per_sec: 30 }),
    dl(3, { bytes_per_sec: 20 }),
  ];
  const frozen = rankOf(start, sort);

  it("holds while speeds change", () => {
    expect(ids(arrangeRows(start, { sort, frozen, pinFailed: false }))).toEqual([2, 3, 1]);
    const later = [
      dl(1, { bytes_per_sec: 99 }),
      dl(2, { bytes_per_sec: 1 }),
      dl(3, { bytes_per_sec: 50 }),
    ];
    expect(ids(arrangeRows(later, { sort, frozen, pinFailed: false }))).toEqual([2, 3, 1]);
  });

  it("puts rows added since the freeze first, newest first", () => {
    const more = [...start, dl(4, { bytes_per_sec: 1 }), dl(5, { bytes_per_sec: 0 })];
    expect(ids(arrangeRows(more, { sort, frozen, pinFailed: false }))).toEqual([5, 4, 2, 3, 1]);
  });

  it("keeps a hidden-then-restored row's rank (ranks come from the full list)", () => {
    const hidden = start.filter((d) => d.id !== 3);
    expect(ids(arrangeRows(hidden, { sort, frozen, pinFailed: false }))).toEqual([2, 1]);
    expect(ids(arrangeRows(start, { sort, frozen, pinFailed: false }))).toEqual([2, 3, 1]);
  });
});

describe("pinning failed rows", () => {
  const ds = [
    snap({ id: 1, status: "failed", added_at: 1 }),
    snap({ id: 2, status: "completed", added_at: 2 }),
    snap({ id: 3, status: "failed", added_at: 3 }),
    snap({ id: 4, status: "downloading", added_at: 4 }),
  ];

  it("moves failed rows to the top, keeping their order, only when asked", () => {
    expect(arrange(ds, DEFAULT_SORT)).toEqual([4, 3, 2, 1]);
    expect(arrange(ds, DEFAULT_SORT, true)).toEqual([3, 1, 4, 2]);
  });

  it("also pins under a frozen order", () => {
    const frozen = rankOf(ds, DEFAULT_SORT);
    expect(ids(arrangeRows(ds, { sort: DEFAULT_SORT, frozen, pinFailed: true }))).toEqual([
      3, 1, 4, 2,
    ]);
  });
});

describe("a finishing row keeps its place (R2)", () => {
  it("keeps its index going downloading → completed under the default sort", () => {
    const rows = (status: DownloadSnapshot["status"]) => [
      snap({ id: 1, status: "completed", added_at: 10 }),
      snap({ id: 2, status, added_at: 20, downloaded: 5, total: 10 }),
      snap({ id: 3, status: "downloading", added_at: 30 }),
    ];
    const before = arrange(rows("downloading"), DEFAULT_SORT, true);
    const after = arrange(rows("completed"), DEFAULT_SORT, true);
    expect(after.indexOf(2)).toBe(before.indexOf(2));
  });
});

describe("waiting rows in queue order", () => {
  const q = (id: number, added_at: number) => queuedSnap({ id, status: "queued", added_at });

  it("fills the waiting rows' slots in the engine's order under any sort", () => {
    // Engine order: 3 waits first, then 1, then 2.
    const ds = [q(3, 30), snap({ id: 9, status: "downloading", added_at: 25 }), q(1, 10), q(2, 20)];
    expect(arrange(ds, DEFAULT_SORT)).toEqual([3, 9, 1, 2]);
    expect(arrange(ds, { key: "added", dir: "asc" })).toEqual([3, 1, 9, 2]);
  });

  it("also under a frozen order", () => {
    const ds = [q(2, 20), q(1, 10)];
    const frozen = new Map([
      [1, 0],
      [2, 1],
    ]);
    expect(ids(arrangeRows(ds, { sort: DEFAULT_SORT, frozen, pinFailed: false }))).toEqual([2, 1]);
  });
});
