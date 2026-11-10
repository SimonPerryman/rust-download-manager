// What Archive finished and Archive take, and their toast text. Archiving is
// reversible, so it happens at once and the toast's Undo unarchives the same
// ids; `useArchive` wires that to the api.

import { isRemovable } from "./rowView";
import type { DownloadSnapshot, Status } from "./types";

/** What Archive finished takes: done and stopped downloads. */
export const ARCHIVE_STATUSES: Status[] = ["completed", "cancelled"];
/** How long the undo toast offers Undo. */
export const UNDO_MS = 5_000;

/** An archive the toast can still undo. */
export interface UndoBatch {
  seq: number;
  ids: number[];
  message: string;
}

/** Finished and not archived yet: Archive applies. */
export function isArchivable(d: DownloadSnapshot): boolean {
  return (
    !d.archived && (d.status === "completed" || d.status === "failed" || d.status === "cancelled")
  );
}

/**
 * What Remove takes from `ds`: archived, finished rows. Remove is for good,
 * so a row resumed or unarchived since it was picked stays.
 */
export function toRemove(ds: readonly DownloadSnapshot[]): DownloadSnapshot[] {
  return ds.filter((d) => d.archived && isRemovable(d));
}

/** The ids Archive finished would take from `ds`. */
export function finishedToArchive(ds: readonly DownloadSnapshot[]): number[] {
  return ds.filter((d) => !d.archived && ARCHIVE_STATUSES.includes(d.status)).map((d) => d.id);
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/** The undo toast's text after archiving `ds`. */
export function archivedMessage(ds: readonly DownloadSnapshot[]): string {
  return ds.length === 1
    ? `Archived ${ds[0].filename}.`
    : `Archived ${plural(ds.length, "download")}.`;
}

/** The toast after Undo. */
export function restoredMessage(count: number): string {
  return count === 1 ? "Restored" : `Restored ${plural(count, "download")}`;
}

/** The toast after Unarchive. */
export function unarchivedMessage(ds: readonly DownloadSnapshot[]): string {
  return ds.length === 1
    ? `Moved ${ds[0].filename} out of the archive.`
    : `Moved ${plural(ds.length, "download")} out of the archive.`;
}
