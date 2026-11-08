// Which table rows are selected: click, Ctrl-click, Shift-click and the
// keyboard's cursor. Pure; ids only, over the rows the view shows now.

export interface Selection {
  selected: ReadonlySet<number>;
  /** Where a Shift range starts. */
  anchor: number | null;
  /** The keyboard's row: arrows move from it, Enter and Alt+arrows act on it. */
  cursor: number | null;
}

export const NO_SELECTION: Selection = { selected: new Set(), anchor: null, cursor: null };

export interface Modifiers {
  ctrl?: boolean;
  shift?: boolean;
}

function range(visible: readonly number[], a: number, b: number): number[] {
  const i = visible.indexOf(a);
  const j = visible.indexOf(b);
  return visible.slice(Math.min(i, j), Math.max(i, j) + 1);
}

/** A click on row `id`: plain selects it alone, Ctrl toggles it, Shift selects the range from the anchor. */
export function clickRow(
  s: Selection,
  id: number,
  mods: Modifiers,
  visible: readonly number[],
): Selection {
  if (mods.shift && s.anchor != null && visible.includes(s.anchor) && visible.includes(id)) {
    const next = new Set(mods.ctrl ? s.selected : []);
    for (const r of range(visible, s.anchor, id)) next.add(r);
    return { selected: next, anchor: s.anchor, cursor: id };
  }
  if (mods.ctrl) {
    const next = new Set(s.selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    return { selected: next, anchor: id, cursor: id };
  }
  return { selected: new Set([id]), anchor: id, cursor: id };
}

/**
 * Moves the cursor to `visible[index]` (clamped), selecting it alone, or with
 * Shift extending the range from the anchor.
 */
export function moveCursor(
  s: Selection,
  index: number,
  shift: boolean,
  visible: readonly number[],
): Selection {
  if (visible.length === 0) return s;
  const id = visible[Math.max(0, Math.min(visible.length - 1, index))];
  return clickRow(s, id, { shift }, visible);
}

/** The cursor's index in `visible`, or -1. */
export function cursorIndex(s: Selection, visible: readonly number[]): number {
  return s.cursor == null ? -1 : visible.indexOf(s.cursor);
}

export function selectAll(s: Selection, visible: readonly number[]): Selection {
  return {
    selected: new Set(visible),
    anchor: s.anchor ?? visible[0] ?? null,
    cursor: s.cursor ?? visible[0] ?? null,
  };
}

/**
 * Drops ids the view no longer shows. The same object comes back when nothing
 * changed, so a caller can skip a re-render.
 */
export function prune(s: Selection, visible: readonly number[]): Selection {
  const shown = new Set(visible);
  const keep = visible.filter((id) => s.selected.has(id));
  const anchor = s.anchor != null && shown.has(s.anchor) ? s.anchor : null;
  const cursor = s.cursor != null && shown.has(s.cursor) ? s.cursor : null;
  if (keep.length === s.selected.size && anchor === s.anchor && cursor === s.cursor) return s;
  return { selected: new Set(keep), anchor, cursor: cursor ?? keep[keep.length - 1] ?? null };
}

/** The selected ids in `visible` (display) order. */
export function selectedInOrder(s: Selection, visible: readonly number[]): number[] {
  return visible.filter((id) => s.selected.has(id));
}
