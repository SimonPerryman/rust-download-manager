// The finish moment's pure parts: spotting downloads that complete while the
// app is watching, and keeping the "finished but never opened" set tidy.

import type { DownloadSnapshot, Status } from "./types";

/** Each download's status as of the last payload seen. */
export type StatusMemo = ReadonlyMap<number, Status>;

/** The fresh tint's length: a 6 s hold (80 %), then a fade to the linger tint. */
export const FRESH_MS = 7_500;
/** How long a finish notification stays up unless hovered or focused. */
export const NOTIFY_MS = 12_000;
export const MAX_NOTIFICATIONS = 3;

/**
 * The ids in `ds` that just completed, and the memo for the next call. The
 * first payload (`prev` null) only seeds; after that a completed id counts as
 * just finished when it was anything else before, or absent (added and
 * completed between two payloads).
 */
export function trackFinishes(
  prev: StatusMemo | null,
  ds: DownloadSnapshot[],
): { memo: StatusMemo; finished: number[] } {
  const memo = new Map(ds.map((d) => [d.id, d.status] as const));
  if (prev === null) return { memo, finished: [] };
  const finished = ds
    .filter((d) => d.status === "completed" && prev.get(d.id) !== "completed")
    .map((d) => d.id);
  return { memo, finished };
}

/** The ids of `unopened` still present in `ds` and completed. */
export function pruneUnopened(unopened: ReadonlySet<number>, ds: DownloadSnapshot[]): Set<number> {
  const completed = new Set(ds.filter((d) => d.status === "completed").map((d) => d.id));
  return new Set([...unopened].filter((id) => completed.has(id)));
}

/** A stored JSON list of ids; bad JSON or a non-list reads as empty, non-integer entries are dropped. */
export function parseIdList(raw: string | null): number[] {
  let value: unknown = null;
  try {
    value = raw == null ? null : JSON.parse(raw);
  } catch {
    return [];
  }
  if (!Array.isArray(value)) return [];
  return value.filter((v): v is number => Number.isInteger(v) && v >= 0);
}
