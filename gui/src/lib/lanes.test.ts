import { describe, expect, it } from "vitest";
import {
  CELL_LANES,
  hasLanes,
  laneRows,
  pieceBox,
  scaleMarks,
  showsCellLanes,
  startsMerge,
} from "./lanes";
import { snap } from "./testSnapshot";
import type { Segment } from "./types";

const seg = (start: number, end: number, done = 0): Segment => ({ start, end, done });
const tiling = (n: number, len = 100): Segment[] =>
  Array.from({ length: n }, (_, i) => seg(i * len, (i + 1) * len));

describe("laneRows", () => {
  it("gives each piece its own line up to the cap", () => {
    const rows = laneRows(tiling(4), CELL_LANES);
    expect(rows).toHaveLength(4);
    expect(rows.map((r) => [r.first, r.last])).toEqual([
      [1, 1],
      [2, 2],
      [3, 3],
      [4, 4],
    ]);
  });

  it("sorts by position in the file, so a stolen tail sits by its neighbours", () => {
    const stolen = [seg(0, 50, 10), seg(100, 200), seg(50, 100)];
    expect(laneRows(stolen, 8).map((r) => r.pieces[0].start)).toEqual([0, 50, 100]);
  });

  it("lets neighbours share a line beyond the cap", () => {
    const rows = laneRows(tiling(12), 8);
    expect(rows.length).toBeLessThanOrEqual(8);
    expect(rows.map((r) => [r.first, r.last])).toEqual([
      [1, 2],
      [3, 4],
      [5, 6],
      [7, 8],
      [9, 10],
      [11, 12],
    ]);
    expect(rows.flatMap((r) => r.pieces)).toHaveLength(12);
  });

  it("never exceeds the cap", () => {
    for (let n = 1; n <= 40; n++) {
      const rows = laneRows(tiling(n), 8);
      expect(rows.length).toBeLessThanOrEqual(8);
      expect(rows.flatMap((r) => r.pieces)).toHaveLength(n);
    }
  });

  it("is empty without a tiling", () => {
    expect(laneRows([], 8)).toEqual([]);
  });
});

describe("hasLanes", () => {
  it("leaves a single-piece download to the plain bar", () => {
    expect(hasLanes(snap({ segments: tiling(1), total: 100 }))).toBe(false);
  });

  it("needs segments and a known size", () => {
    expect(hasLanes(snap({ segments: tiling(4), total: 400 }))).toBe(true);
    expect(hasLanes(snap({ segments: [], total: 400 }))).toBe(false);
    expect(hasLanes(snap({ segments: tiling(4), total: null }))).toBe(false);
  });
});

describe("scaleMarks", () => {
  it("marks quarters of the file", () => {
    expect(scaleMarks(4 * 1024 * 1024)).toEqual(["0", "1.00 MB", "2.00 MB", "3.00 MB", "4.00 MB"]);
  });
});

describe("pieceBox", () => {
  it("places a piece and its fetched part along the file", () => {
    expect(pieceBox(seg(100, 200, 50), 400)).toEqual({ left: 25, width: 25, fill: 12.5 });
  });

  it("never draws the fill past the piece", () => {
    expect(pieceBox(seg(0, 100, 150), 100).fill).toBe(100);
  });
});

describe("showsCellLanes", () => {
  const base = { segments: tiling(4), total: 400 };

  it("draws lanes while moving or resumable", () => {
    expect(showsCellLanes(snap({ ...base, status: "downloading" }))).toBe(true);
    expect(showsCellLanes(snap({ ...base, status: "paused" }))).toBe(true);
    expect(showsCellLanes(snap({ ...base, status: "cancelled", partial_kept: true }))).toBe(true);
  });

  it("leaves the quiet words to every other row", () => {
    for (const status of ["queued", "completed", "failed"] as const) {
      expect(showsCellLanes(snap({ ...base, status }))).toBe(false);
    }
    expect(showsCellLanes(snap({ ...base, status: "cancelled" }))).toBe(false);
  });
});

describe("startsMerge", () => {
  const done = snap({ status: "completed", segments: tiling(4, 100), total: 400 });

  it("merges a row that showed lanes and just finished", () => {
    expect(startsMerge(true, done)).toBe(true);
  });

  it("never merges on a reload or without pieces", () => {
    expect(startsMerge(false, done)).toBe(false);
    expect(startsMerge(true, snap({ status: "completed", total: 400 }))).toBe(false);
  });

  it("only merges into Completed", () => {
    expect(startsMerge(true, snap({ status: "failed", segments: tiling(4), total: 400 }))).toBe(
      false,
    );
  });
});
