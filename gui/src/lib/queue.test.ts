import { describe, expect, it } from "vitest";
import { dropIndex, nearestDrop, queueOrder, stepIndex } from "./queue";
import { snap } from "./testSnapshot";

describe("queueOrder", () => {
  it("lists queued downloads in the engine's order", () => {
    const ds = [
      snap({ id: 5 }),
      snap({ id: 1, status: "downloading" }),
      snap({ id: 3 }),
      snap({ id: 2, status: "paused" }),
      snap({ id: 4 }),
    ];
    expect(queueOrder(ds)).toEqual([5, 3, 4]);
  });
});

describe("stepIndex", () => {
  const q = [7, 8, 9];
  it("moves one place up or down", () => {
    expect(stepIndex(q, 9, -1)).toBe(1);
    expect(stepIndex(q, 7, 1)).toBe(1);
  });
  it("can't move past either end or a row that isn't waiting", () => {
    expect(stepIndex(q, 7, -1)).toBeNull();
    expect(stepIndex(q, 9, 1)).toBeNull();
    expect(stepIndex(q, 1, -1)).toBeNull();
  });
});

describe("dropIndex", () => {
  const q = [1, 2, 3, 4];
  it("drops before or after the target", () => {
    expect(dropIndex(q, 4, 1, false)).toBe(0);
    expect(dropIndex(q, 4, 1, true)).toBe(1);
    expect(dropIndex(q, 1, 4, true)).toBe(3);
    expect(dropIndex(q, 1, 3, false)).toBe(1);
  });
  it("is no move when it lands where it was", () => {
    expect(dropIndex(q, 2, 3, false)).toBeNull();
    expect(dropIndex(q, 2, 1, true)).toBeNull();
    expect(dropIndex(q, 2, 2, true)).toBeNull();
    expect(dropIndex(q, 9, 1, false)).toBeNull();
  });
});

describe("nearestDrop", () => {
  const rows = [
    { id: 1, top: 0, height: 32 },
    { id: 2, top: 32, height: 32 },
    { id: 3, top: 64, height: 32 },
  ];
  it("picks the nearest other row and the half the pointer is in", () => {
    expect(nearestDrop(rows, 3, 5)).toEqual({ id: 1, after: false });
    expect(nearestDrop(rows, 3, 30)).toEqual({ id: 1, after: true });
    expect(nearestDrop(rows, 1, 50)).toEqual({ id: 2, after: true });
  });
  it("skips the dragged row itself", () => {
    expect(nearestDrop(rows, 2, 48)?.id).not.toBe(2);
  });
  it("is null with no other row", () => {
    expect(nearestDrop([rows[0]], 1, 10)).toBeNull();
  });
});
