import { useCallback, useRef, useState } from "react";
import { nearestDrop, type DropTarget } from "../../lib/queue";

/** A pointer must travel this far before a press on the grip counts as a drag. */
const DRAG_THRESHOLD_PX = 4;

export interface QueueDrag {
  /** The row being dragged, once the pointer has moved. */
  dragging: number | null;
  /** The waiting row it would drop beside. */
  over: DropTarget | null;
  onGripDown: (e: React.PointerEvent<HTMLButtonElement>, id: number) => void;
}

/**
 * Drag a waiting row by its grip onto another waiting row; `onDrop(id, target,
 * after)` fires on release. Rows are found by their `st-waiting` class and
 * `data-id` within the grip's table body.
 */
export function useQueueDrag(
  onDrop: (id: number, target: number, after: boolean) => void,
): QueueDrag {
  const [dragging, setDragging] = useState<number | null>(null);
  const [over, setOver] = useState<DropTarget | null>(null);
  const onDropRef = useRef(onDrop);
  onDropRef.current = onDrop;

  const onGripDown = useCallback((e: React.PointerEvent<HTMLButtonElement>, id: number) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const body = e.currentTarget.closest("tbody");
    const startY = e.clientY;
    let moved = false;
    let target: DropTarget | null = null;

    const waitingRows = () =>
      Array.from(body?.querySelectorAll<HTMLElement>("tr.st-waiting[data-id]") ?? []).map((row) => {
        const r = row.getBoundingClientRect();
        return { id: Number(row.dataset.id), top: r.top, height: r.height };
      });
    // On the window, not the grip: the grip unmounts if its row starts mid-drag.
    const move = (ev: PointerEvent) => {
      if (!moved && Math.abs(ev.clientY - startY) <= DRAG_THRESHOLD_PX) return;
      if (!moved) {
        moved = true;
        setDragging(id);
      }
      // The dragged row left the queue (it started): there's nothing to move.
      const rows = waitingRows();
      if (!rows.some((r) => r.id === id)) return finish(false);
      target = nearestDrop(rows, id, ev.clientY);
      setOver(target);
    };
    const up = () => finish(true);
    const cancel = () => finish(false);
    // Escape cancels just the drag; the table would also clear the selection.
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key !== "Escape") return;
      ev.stopPropagation();
      ev.preventDefault();
      finish(false);
    };
    const finish = (drop: boolean) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("blur", cancel);
      window.removeEventListener("keydown", onKey, true);
      setDragging(null);
      setOver(null);
      if (drop && moved && target) onDropRef.current(id, target.id, target.after);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("blur", cancel);
    window.addEventListener("keydown", onKey, true);
  }, []);

  return { dragging, over, onGripDown };
}
