// "Clear finished" with an undo window, as a pure state machine: rows hide at
// click time, and only when the window closes is `clear` sent, for exactly the
// ids captured at the click. `usePendingClear` wires the effects to a timer
// and the api.

import type { DownloadSnapshot, Status } from "./types";

/** What Clear finished removes: done and stopped downloads. */
export const CLEAR_STATUSES: Status[] = ["completed", "cancelled"];
export const UNDO_MS = 5_000;

/** One click's worth of rows; `kept` counts stopped rows whose partial file is kept. */
export interface Batch {
  seq: number;
  ids: number[];
  kept: number;
  message: string;
}

export interface ClearState {
  /** In its undo window. */
  pending: Batch | null;
  /** `clear` sent; rows stay hidden until a snapshot drops them. */
  committing: Batch[];
  /** Every pending and committing id. */
  hidden: ReadonlySet<number>;
  nextSeq: number;
}

export type ClearEvent =
  /** Plans a batch from these (shown) rows. */
  | { type: "click"; downloads: DownloadSnapshot[] }
  | { type: "undo" }
  | { type: "timeout"; seq: number }
  /** `clear` rejected for this batch. */
  | { type: "failed"; seq: number }
  | { type: "snapshot"; downloads: DownloadSnapshot[] };

export type ClearEffect =
  /** Call `clear(CLEAR_STATUSES, batch.ids)`. */
  | { type: "commit"; batch: Batch }
  /** Start the `UNDO_MS` timer (replacing any running one). */
  | { type: "arm"; seq: number }
  | { type: "disarm" };

export const INITIAL_CLEAR: ClearState = {
  pending: null,
  committing: [],
  hidden: new Set(),
  nextSeq: 0,
};

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/** The undo toast's text. */
export function clearMessage(count: number, kept: number): string {
  const stay = kept > 0 ? "Files and kept partial files stay on disk." : "Files stay on disk.";
  return `Cleared ${plural(count, "finished download")}. ${stay}`;
}

/** The toast after Undo. */
export function restoredMessage(count: number): string {
  return count === 1 ? "Restored" : `Restored ${plural(count, "download")}`;
}

/** The batch Clear finished would take from `ds`, or null when there's nothing to clear. */
export function planBatch(ds: DownloadSnapshot[], seq: number): Batch | null {
  const rows = ds.filter((d) => CLEAR_STATUSES.includes(d.status));
  if (rows.length === 0) return null;
  const kept = rows.filter((d) => d.status === "cancelled" && d.partial_kept).length;
  return { seq, ids: rows.map((d) => d.id), kept, message: clearMessage(rows.length, kept) };
}

function withBatches(s: ClearState, pending: Batch | null, committing: Batch[]): ClearState {
  const hidden = new Set([...(pending?.ids ?? []), ...committing.flatMap((b) => b.ids)]);
  return { ...s, pending, committing, hidden };
}

type Result = { state: ClearState; effects: ClearEffect[] };

export function reduceClear(s: ClearState, e: ClearEvent): Result {
  const same: Result = { state: s, effects: [] };
  switch (e.type) {
    case "click": {
      const batch = planBatch(e.downloads, s.nextSeq);
      if (!batch) return same;
      const effects: ClearEffect[] = [];
      let committing = s.committing;
      if (s.pending) {
        committing = [...committing, s.pending];
        effects.push({ type: "commit", batch: s.pending });
      }
      effects.push({ type: "arm", seq: batch.seq });
      return { state: { ...withBatches(s, batch, committing), nextSeq: s.nextSeq + 1 }, effects };
    }
    case "undo":
      if (!s.pending) return same;
      return { state: withBatches(s, null, s.committing), effects: [{ type: "disarm" }] };
    case "timeout": {
      const { pending } = s;
      if (!pending || pending.seq !== e.seq) return same;
      return {
        state: withBatches(s, null, [...s.committing, pending]),
        effects: [{ type: "commit", batch: pending }],
      };
    }
    case "failed": {
      const committing = s.committing.filter((b) => b.seq !== e.seq);
      if (committing.length === s.committing.length) return same;
      return { state: withBatches(s, s.pending, committing), effects: [] };
    }
    case "snapshot": {
      const present = new Set(e.downloads.map((d) => d.id));
      let changed = false;
      const committing = s.committing.flatMap((b) => {
        const ids = b.ids.filter((id) => present.has(id));
        if (ids.length === b.ids.length) return [b];
        changed = true;
        return ids.length ? [{ ...b, ids }] : [];
      });
      return changed ? { state: withBatches(s, s.pending, committing), effects: [] } : same;
    }
  }
}
