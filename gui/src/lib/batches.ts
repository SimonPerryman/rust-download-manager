// Batches as the list shows them: each batch's files rolled up for its group
// row and the sidebar, and the table's row layout with those rows gathered
// under it. Pure presentation over snapshots; the engine owns what a batch is.

import type { DownloadSnapshot } from "./types";

/** One batch's working (not archived) files and how they are doing. */
export interface BatchGroup {
  id: number;
  label: string;
  items: DownloadSnapshot[];
  /** Files completed. */
  done: number;
  /** Files failed. */
  failed: number;
  /** 0 to 1: finished files count whole, the rest by their bytes so far. */
  fraction: number;
}

function share(d: DownloadSnapshot): number {
  if (d.status === "completed") return 1;
  return d.total ? Math.min(d.downloaded / d.total, 1) : 0;
}

/** Every batch among `ds`'s working files, ordered by name then id. */
export function batchGroups(ds: readonly DownloadSnapshot[]): BatchGroup[] {
  const byId = new Map<number, DownloadSnapshot[]>();
  for (const d of ds) {
    if (d.archived || d.batch == null) continue;
    const items = byId.get(d.batch);
    if (items) items.push(d);
    else byId.set(d.batch, [d]);
  }
  return [...byId.entries()]
    .map(([id, items]) => ({
      id,
      label: items[0].batch_label ?? "Batch",
      items,
      done: items.filter((d) => d.status === "completed").length,
      failed: items.filter((d) => d.status === "failed").length,
      fraction: items.reduce((sum, d) => sum + share(d), 0) / items.length,
    }))
    .sort(
      (a, b) => a.label.localeCompare(b.label, undefined, { sensitivity: "base" }) || a.id - b.id,
    );
}

/** The group row's rollup: "9 of 14 done". */
export function rollupText(g: BatchGroup): string {
  return `${g.done} of ${g.items.length} done`;
}

/** The "2 failed" pill. */
export function failedText(g: BatchGroup): string {
  return `${g.failed} failed`;
}

/** What the table lists: a batch's group row, or one download. */
export type TableItem =
  | { kind: "group"; group: BatchGroup; collapsed: boolean }
  | { kind: "row"; d: DownloadSnapshot; inBatch: boolean };

/**
 * The table's items for `rows` (already filtered and sorted). With `grouped`, each
 * batch's rows gather under one group row placed where its first row was, and a
 * collapsed batch lists only that row. `shown` is the downloads listed, in order:
 * what selection and the keyboard move over.
 */
export function layoutRows(
  rows: readonly DownloadSnapshot[],
  groups: readonly BatchGroup[],
  grouped: boolean,
  collapsed: ReadonlySet<number>,
): { items: TableItem[]; shown: DownloadSnapshot[] } {
  if (!grouped) {
    return { items: rows.map((d) => ({ kind: "row", d, inBatch: false })), shown: [...rows] };
  }
  const byId = new Map(groups.map((g) => [g.id, g]));
  const gathered = new Map<number, DownloadSnapshot[]>();
  for (const d of rows) {
    if (d.batch != null && byId.has(d.batch)) {
      const list = gathered.get(d.batch);
      if (list) list.push(d);
      else gathered.set(d.batch, [d]);
    }
  }
  const items: TableItem[] = [];
  const shown: DownloadSnapshot[] = [];
  const placed = new Set<number>();
  for (const d of rows) {
    const group = d.batch == null ? undefined : byId.get(d.batch);
    if (!group) {
      items.push({ kind: "row", d, inBatch: false });
      shown.push(d);
      continue;
    }
    if (placed.has(group.id)) continue;
    placed.add(group.id);
    const isCollapsed = collapsed.has(group.id);
    items.push({ kind: "group", group, collapsed: isCollapsed });
    if (isCollapsed) continue;
    for (const member of gathered.get(group.id)!) {
      items.push({ kind: "row", d: member, inBatch: true });
      shown.push(member);
    }
  }
  return { items, shown };
}

const NAME_TRIM = /[\s\-_.]+$/;

/** A batch name for a pattern's stem (`lesson-` gives `lesson`), within the engine's length. */
export function batchNameFromStem(stem: string): string {
  return [...stem.trim().replace(NAME_TRIM, "")].slice(0, MAX_BATCH_NAME).join("");
}

/** The engine's `MAX_BATCH_LABEL_LEN`: longest batch name, in characters. */
export const MAX_BATCH_NAME = 64;

/** The last part of a folder path: "Downloads" for `C:\Users\Simon\Downloads`. */
export function folderName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? "";
}

/** The folder tick's label. */
export function folderTickLabel(name: string, dest: string): string {
  return `Create folder “${name.trim()}” in ${folderName(dest) || "Downloads"}`;
}

/** The toast after Retry failed on a group row. */
export function retryBatchMessage(n: number): string {
  return n === 1 ? "Retrying 1 failed file" : `Retrying ${n} failed files`;
}
