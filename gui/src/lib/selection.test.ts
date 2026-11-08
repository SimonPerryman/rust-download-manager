import { describe, expect, it } from "vitest";
import {
  clickRow,
  cursorIndex,
  moveCursor,
  NO_SELECTION,
  prune,
  selectAll,
  selectedInOrder,
  type Selection,
} from "./selection";

const visible = [10, 20, 30, 40, 50];
const ids = (s: Selection) => [...s.selected].sort((a, b) => a - b);

describe("clickRow", () => {
  it("selects one row alone and makes it the anchor and cursor", () => {
    const s = clickRow(clickRow(NO_SELECTION, 10, {}, visible), 30, {}, visible);
    expect(ids(s)).toEqual([30]);
    expect(s).toMatchObject({ anchor: 30, cursor: 30 });
  });

  it("toggles with Ctrl", () => {
    let s = clickRow(NO_SELECTION, 10, {}, visible);
    s = clickRow(s, 30, { ctrl: true }, visible);
    expect(ids(s)).toEqual([10, 30]);
    s = clickRow(s, 10, { ctrl: true }, visible);
    expect(ids(s)).toEqual([30]);
    expect(s.anchor).toBe(10);
  });

  it("selects the range from the anchor with Shift, in either direction", () => {
    const a = clickRow(NO_SELECTION, 20, {}, visible);
    expect(ids(clickRow(a, 40, { shift: true }, visible))).toEqual([20, 30, 40]);
    expect(ids(clickRow(a, 10, { shift: true }, visible))).toEqual([10, 20]);
    expect(clickRow(a, 40, { shift: true }, visible)).toMatchObject({ anchor: 20, cursor: 40 });
  });

  it("adds the range to the selection with Ctrl+Shift", () => {
    let s = clickRow(NO_SELECTION, 50, {}, visible);
    s = clickRow(s, 10, { ctrl: true }, visible);
    s = clickRow(s, 30, { ctrl: true, shift: true }, visible);
    expect(ids(s)).toEqual([10, 20, 30, 50]);
  });

  it("treats Shift with no visible anchor as a plain click", () => {
    expect(ids(clickRow(NO_SELECTION, 30, { shift: true }, visible))).toEqual([30]);
  });
});

describe("moveCursor", () => {
  it("moves and selects, clamped to the rows", () => {
    const s = clickRow(NO_SELECTION, 20, {}, visible);
    expect(moveCursor(s, cursorIndex(s, visible) + 1, false, visible)).toMatchObject({
      cursor: 30,
    });
    expect(ids(moveCursor(s, 99, false, visible))).toEqual([50]);
    expect(ids(moveCursor(s, -1, false, visible))).toEqual([10]);
  });

  it("extends from the anchor with Shift", () => {
    let s = clickRow(NO_SELECTION, 20, {}, visible);
    s = moveCursor(s, 3, true, visible);
    s = moveCursor(s, 2, true, visible);
    expect(ids(s)).toEqual([20, 30]);
  });

  it("does nothing with no rows", () => {
    expect(moveCursor(NO_SELECTION, 0, false, [])).toBe(NO_SELECTION);
  });
});

describe("selectAll / prune / selectedInOrder", () => {
  it("selects every visible row", () => {
    expect(ids(selectAll(NO_SELECTION, visible))).toEqual(visible);
    expect(selectAll(NO_SELECTION, visible).cursor).toBe(10);
  });

  it("drops rows the view stopped showing and moves the cursor to one that's left", () => {
    let s = clickRow(NO_SELECTION, 20, {}, visible);
    s = clickRow(s, 40, { ctrl: true }, visible);
    const p = prune(s, [10, 20, 30]);
    expect(ids(p)).toEqual([20]);
    expect(p).toMatchObject({ cursor: 20, anchor: null });
  });

  it("returns the same object when nothing changed", () => {
    const s = clickRow(NO_SELECTION, 20, {}, visible);
    expect(prune(s, visible)).toBe(s);
  });

  it("lists the selection in display order", () => {
    let s = clickRow(NO_SELECTION, 40, {}, visible);
    s = clickRow(s, 10, { ctrl: true }, visible);
    expect(selectedInOrder(s, visible)).toEqual([10, 40]);
  });
});
