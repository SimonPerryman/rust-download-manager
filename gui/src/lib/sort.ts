// Table sorting: the sortable columns' values, header-click cycling, and the
// row order (with a frozen order for columns whose values move every tick, so
// rows don't jump under the pointer). Pure presentation over snapshots.

import type { DownloadSnapshot } from "./types";

export type SortKey = "name" | "size" | "progress" | "speed" | "left" | "connections" | "added";

export interface SortState {
  key: SortKey;
  dir: "asc" | "desc";
}

/** Newest added first. */
export const DEFAULT_SORT: SortState = { key: "added", dir: "desc" };

/** A header click: the active key flips direction; a new key starts name asc, others desc. */
export function nextSort(current: SortState, clicked: SortKey): SortState {
  if (current.key === clicked) return { key: clicked, dir: current.dir === "asc" ? "desc" : "asc" };
  return { key: clicked, dir: clicked === "name" ? "asc" : "desc" };
}

const VOLATILE: ReadonlySet<SortKey> = new Set(["progress", "speed", "left", "connections"]);

/** Whether `key`'s values change while downloading (so its order is frozen at click time). */
export function isVolatile(key: SortKey): boolean {
  return VOLATILE.has(key);
}

const names = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

const live = (d: DownloadSnapshot, v: number | null) => (d.status === "downloading" ? v : null);

/** The value `key` sorts `d` by; null when unknown. Size is `total`, which never moves. */
function valueOf(key: Exclude<SortKey, "name">, d: DownloadSnapshot): number | null {
  switch (key) {
    case "size":
      return d.total;
    case "progress":
      if (d.status === "completed") return 2;
      return d.total ? d.downloaded / d.total : null;
    case "speed":
      return live(d, d.bytes_per_sec);
    case "left":
      return live(d, d.eta_secs);
    case "connections":
      return live(d, d.connections);
    case "added":
      return d.added_at;
  }
}

function comparator({ key, dir }: SortState): (a: DownloadSnapshot, b: DownloadSnapshot) => number {
  const sign = dir === "asc" ? 1 : -1;
  return (a, b) => {
    let c: number;
    if (key === "name") {
      c = sign * names.compare(a.filename, b.filename);
    } else {
      const va = valueOf(key, a);
      const vb = valueOf(key, b);
      // Unknown values sort last whatever the direction.
      if (va == null || vb == null) c = va == null ? (vb == null ? 0 : 1) : -1;
      else c = sign * (va - vb);
    }
    return c || b.id - a.id;
  };
}

/** Ascending by `key`: unknown values last, ties newest (highest id) first. */
export function compareBy(key: SortKey): (a: DownloadSnapshot, b: DownloadSnapshot) => number {
  return comparator({ key, dir: "asc" });
}

/** Each id's position under `sort`, for freezing the order. */
export function rankOf(ds: DownloadSnapshot[], sort: SortState): Map<number, number> {
  return new Map([...ds].sort(comparator(sort)).map((d, i) => [d.id, i]));
}

/**
 * The table's row order: by `sort`, or by `frozen` rank when given (rows added
 * since the freeze go first, newest first); `pinFailed` lifts failed rows to
 * the top, keeping their order. Waiting rows keep the slots the order gives
 * them but fill those slots in `ds`'s own order (the engine's queue), so a
 * reorder shows under any sort.
 */
export function arrangeRows(
  ds: DownloadSnapshot[],
  opts: { sort: SortState; frozen: Map<number, number> | null; pinFailed: boolean },
): DownloadSnapshot[] {
  const { frozen } = opts;
  const order = frozen
    ? (a: DownloadSnapshot, b: DownloadSnapshot) => {
        const ra = frozen.get(a.id);
        const rb = frozen.get(b.id);
        if (ra == null || rb == null) return ra == null ? (rb == null ? b.id - a.id : -1) : 1;
        return ra - rb;
      }
    : comparator(opts.sort);
  const sorted = [...ds].sort(order);
  const queue = ds.filter((d) => d.status === "queued");
  let next = 0;
  const rows = sorted.map((d) => (d.status === "queued" ? queue[next++] : d));
  if (!opts.pinFailed) return rows;
  return [
    ...rows.filter((d) => d.status === "failed"),
    ...rows.filter((d) => d.status !== "failed"),
  ];
}
