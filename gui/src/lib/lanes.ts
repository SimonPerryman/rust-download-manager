// The table cell's lanes: one hairline per piece of the file (the engine's
// segment tiling), drawn across the whole file so each line shows where its
// piece sits and how far it has got. Pure; the Lanes component only renders
// what this returns. (The details pane draws one row per connection instead;
// see laneWork.ts.)

import { formatBytes } from "./format";
import type { DownloadSnapshot, Segment } from "./types";

/** A 32 px table row fits 8 hairlines. */
export const CELL_LANES = 8;

/** One drawn line: pieces `first`..`last` (1-based, in file order) sharing it. */
export interface LaneRow {
  first: number;
  last: number;
  pieces: Segment[];
}

const byStart = (a: Segment, b: Segment) => a.start - b.start;

/** The tiling as at most `max` lines; past that, neighbouring pieces share a line. */
export function laneRows(segments: readonly Segment[], max: number): LaneRow[] {
  const sorted = [...segments].sort(byStart);
  const per = Math.max(1, Math.ceil(sorted.length / Math.max(1, max)));
  const rows: LaneRow[] = [];
  for (let i = 0; i < sorted.length; i += per) {
    const pieces = sorted.slice(i, i + per);
    rows.push({ first: i + 1, last: i + pieces.length, pieces });
  }
  return rows;
}

const pieceLen = (s: Segment) => Math.max(0, s.end - s.start);

/**
 * Whether the download has lanes worth drawing: more than one piece (a
 * single-connection download keeps the plain bar) and a size to scale them to.
 */
export function hasLanes(d: DownloadSnapshot): boolean {
  return d.segments.length > 1 && (d.total ?? 0) > 0;
}

/** Whether the table's Progress cell draws lanes: moving, or resumable from where it is. */
export function showsCellLanes(d: DownloadSnapshot): boolean {
  if (!hasLanes(d)) return false;
  return (
    d.status === "downloading" ||
    d.status === "paused" ||
    (d.status === "cancelled" && d.partial_kept)
  );
}

/**
 * Whether the cell merges its lanes into one bar: it showed lanes on the last
 * render and has just finished with its pieces still known (so not on a reload).
 */
export function startsMerge(showedLanes: boolean, d: DownloadSnapshot): boolean {
  return showedLanes && d.status === "completed" && hasLanes(d);
}

/** Labels under the pane's lanes: 0, a quarter, half, three quarters and the whole size. */
export function scaleMarks(total: number): string[] {
  return [0, 0.25, 0.5, 0.75, 1].map((k) => (k === 0 ? "0" : formatBytes(Math.round(total * k))));
}

/** Where a piece and its fetched part sit along the file, as percentages. */
export function pieceBox(s: Segment, total: number): { left: number; width: number; fill: number } {
  const t = Math.max(1, total);
  return {
    left: (s.start / t) * 100,
    width: (pieceLen(s) / t) * 100,
    fill: (Math.min(s.done, pieceLen(s)) / t) * 100,
  };
}
