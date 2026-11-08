import { useCallback, useEffect, useRef, useState } from "react";
import {
  clickRow,
  cursorIndex,
  moveCursor,
  NO_SELECTION,
  prune,
  selectAll,
  type Modifiers,
  type Selection,
} from "./selection";

export interface SelectionApi {
  selection: Selection;
  click: (id: number, mods: Modifiers) => void;
  /** Moves the cursor up, down, or to the first or last row; returns the new cursor id. */
  move: (to: "up" | "down" | "first" | "last", extend: boolean) => number | null;
  selectAll: () => void;
  clear: () => void;
}

/**
 * The table's selection over `visible` (display order). Rows that leave the
 * view (a filter change, a removal) leave the selection too.
 */
export function useSelection(visible: readonly number[]): SelectionApi {
  const [selection, setSelection] = useState<Selection>(NO_SELECTION);
  const ref = useRef(selection);
  const visibleRef = useRef(visible);
  visibleRef.current = visible;

  const set = useCallback((next: Selection) => {
    ref.current = next;
    setSelection(next);
  }, []);

  useEffect(() => {
    const next = prune(ref.current, visible);
    if (next !== ref.current) set(next);
  }, [visible, set]);

  const click = useCallback(
    (id: number, mods: Modifiers) => set(clickRow(ref.current, id, mods, visibleRef.current)),
    [set],
  );
  const move = useCallback(
    (to: "up" | "down" | "first" | "last", extend: boolean) => {
      const rows = visibleRef.current;
      const i = cursorIndex(ref.current, rows);
      const index =
        to === "first"
          ? 0
          : to === "last"
            ? rows.length - 1
            : i < 0
              ? 0
              : i + (to === "up" ? -1 : 1);
      const next = moveCursor(ref.current, index, extend, rows);
      set(next);
      return next.cursor;
    },
    [set],
  );
  const all = useCallback(() => set(selectAll(ref.current, visibleRef.current)), [set]);
  const clear = useCallback(() => set(NO_SELECTION), [set]);

  return { selection, click, move, selectAll: all, clear };
}
