// The waiting queue as the table shows it: the engine's order of queued
// downloads (its snapshot order), and where a reorder moves one. The move
// itself is the engine's `move_queued`.

import type { DownloadSnapshot } from "./types";

/** Ids of the waiting downloads, first to start first. */
export function queueOrder(ds: readonly DownloadSnapshot[]): number[] {
  return ds.filter((d) => d.status === "queued").map((d) => d.id);
}

/** The index Alt+↑ (`delta` -1) or Alt+↓ (+1) moves `id` to, or null when it can't move. */
export function stepIndex(queue: readonly number[], id: number, delta: number): number | null {
  const i = queue.indexOf(id);
  if (i < 0) return null;
  const j = Math.max(0, Math.min(queue.length - 1, i + delta));
  return j === i ? null : j;
}

/**
 * The index a drag of `id` dropped before (or `after`) `target` moves it to,
 * counted among the waiting downloads once `id` is taken out; null for no move.
 */
export function dropIndex(
  queue: readonly number[],
  id: number,
  target: number,
  after: boolean,
): number | null {
  if (id === target || !queue.includes(id)) return null;
  const rest = queue.filter((q) => q !== id);
  const t = rest.indexOf(target);
  if (t < 0) return null;
  const j = t + (after ? 1 : 0);
  return j === queue.indexOf(id) ? null : j;
}

/** Where a dragged waiting row would land: beside `id`, before it or `after`. */
export interface DropTarget {
  id: number;
  after: boolean;
}

/**
 * The waiting row nearest the pointer at `y` (other than the dragged `id`),
 * and which half of it the pointer is in; null when there's no other row.
 */
export function nearestDrop(
  rows: readonly { id: number; top: number; height: number }[],
  id: number,
  y: number,
): DropTarget | null {
  let best: (DropTarget & { distance: number }) | null = null;
  for (const r of rows) {
    if (r.id === id) continue;
    const mid = r.top + r.height / 2;
    const distance = Math.abs(y - mid);
    if (!best || distance < best.distance) best = { id: r.id, after: y > mid, distance };
  }
  return best && { id: best.id, after: best.after };
}
