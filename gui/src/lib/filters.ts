// The sidebar's view filters: which rows each status, folder, Archived or tag
// filter shows, the name filter, and the live counts. Archived rows show only
// in the Archived views. Pure presentation over snapshots.

import { folderOf } from "./folders";
import { host } from "./format";
import { hasTag, tagKey } from "./tags";
import type { DownloadSnapshot, Status } from "./types";

export type StatusFilter =
  | "all"
  | "downloading"
  | "waiting"
  | "paused"
  | "done"
  | "failed"
  | "stopped";

export type ViewFilter =
  | { kind: "status"; status: StatusFilter }
  | { kind: "folder"; dir: string }
  | { kind: "archived" }
  /** A tag, among the archived rows ("Archived › tag") or the rest. */
  | { kind: "tag"; tag: string; archived: boolean };

/** Whether `f` shows archived rows: Archived, or a tag under it. */
export function isArchivedView(f: ViewFilter): boolean {
  return f.kind === "archived" || (f.kind === "tag" && f.archived);
}

/** The status filters in sidebar order, with their labels. */
export const STATUS_FILTERS: readonly { key: StatusFilter; label: string }[] = [
  { key: "all", label: "All" },
  { key: "downloading", label: "Downloading" },
  { key: "waiting", label: "Queued" },
  { key: "paused", label: "Paused" },
  { key: "done", label: "Completed" },
  { key: "failed", label: "Failed" },
  { key: "stopped", label: "Stopped" },
];

const KEY_OF: Record<Status, Exclude<StatusFilter, "all">> = {
  queued: "waiting",
  downloading: "downloading",
  paused: "paused",
  completed: "done",
  failed: "failed",
  cancelled: "stopped",
};

/** The one status filter (besides All) an engine status belongs to. */
export function filterKeyOf(status: Status): Exclude<StatusFilter, "all"> {
  return KEY_OF[status];
}

export function matchesFilter(d: DownloadSnapshot, f: ViewFilter): boolean {
  if (d.archived !== isArchivedView(f)) return false;
  switch (f.kind) {
    case "archived":
      return true;
    case "tag":
      return hasTag(d, f.tag);
    case "folder":
      return folderOf(d.target) === f.dir;
    case "status":
      return f.status === "all" || filterKeyOf(d.status) === f.status;
  }
}

/** Whether the filename or host contains `query` (trimmed, case-insensitive); blank matches all. */
export function matchesQuery(d: DownloadSnapshot, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return d.filename.toLowerCase().includes(q) || host(d.url).toLowerCase().includes(q);
}

/** How many of `ds` each status filter shows (archived rows are in none). */
export function statusCounts(ds: readonly DownloadSnapshot[]): Record<StatusFilter, number> {
  const live = ds.filter((d) => !d.archived);
  const counts: Record<StatusFilter, number> = {
    all: live.length,
    downloading: 0,
    waiting: 0,
    paused: 0,
    done: 0,
    failed: 0,
    stopped: 0,
  };
  for (const d of live) counts[filterKeyOf(d.status)] += 1;
  return counts;
}

/** How many rows the Archived view shows. */
export function archivedCount(ds: readonly DownloadSnapshot[]): number {
  return ds.filter((d) => d.archived).length;
}

export function sameFilter(a: ViewFilter, b: ViewFilter): boolean {
  switch (a.kind) {
    case "status":
      return b.kind === "status" && a.status === b.status;
    case "folder":
      return b.kind === "folder" && a.dir === b.dir;
    case "archived":
      return b.kind === "archived";
    case "tag":
      return b.kind === "tag" && a.archived === b.archived && tagKey(a.tag) === tagKey(b.tag);
  }
}
