// The sidebar's view filters: which rows each status or folder filter shows,
// the name filter, and the live counts. Pure presentation over snapshots.

import { folderOf } from "./folders";
import { host } from "./format";
import type { DownloadSnapshot, Status } from "./types";

export type StatusFilter =
  | "all"
  | "downloading"
  | "waiting"
  | "paused"
  | "done"
  | "failed"
  | "stopped";

export type ViewFilter = { kind: "status"; status: StatusFilter } | { kind: "folder"; dir: string };

/** The status filters in sidebar order, with their labels. */
export const STATUS_FILTERS: readonly { key: StatusFilter; label: string }[] = [
  { key: "all", label: "All" },
  { key: "downloading", label: "Downloading" },
  { key: "waiting", label: "Waiting" },
  { key: "paused", label: "Paused" },
  { key: "done", label: "Done" },
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
  if (f.kind === "folder") return folderOf(d.target) === f.dir;
  return f.status === "all" || filterKeyOf(d.status) === f.status;
}

/** Whether the filename or host contains `query` (trimmed, case-insensitive); blank matches all. */
export function matchesQuery(d: DownloadSnapshot, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return d.filename.toLowerCase().includes(q) || host(d.url).toLowerCase().includes(q);
}

/** How many of `ds` each status filter shows. */
export function statusCounts(ds: DownloadSnapshot[]): Record<StatusFilter, number> {
  const counts: Record<StatusFilter, number> = {
    all: ds.length,
    downloading: 0,
    waiting: 0,
    paused: 0,
    done: 0,
    failed: 0,
    stopped: 0,
  };
  for (const d of ds) counts[filterKeyOf(d.status)] += 1;
  return counts;
}

export function sameFilter(a: ViewFilter, b: ViewFilter): boolean {
  if (a.kind === "status") return b.kind === "status" && a.status === b.status;
  return b.kind === "folder" && a.dir === b.dir;
}
