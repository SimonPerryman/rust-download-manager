// The window's words: view titles, empty states, dialog hints and toast text.
// Sentence case, user-side verbs, and an action keeps its name through the flow
// ("Stop" leads to "Stopped", "Remove" to "Removed", "Archive" to "Archived").

import { formatBytes, formatSpeed } from "./format";
import { STATUS_FILTERS, type ViewFilter } from "./filters";
import type { Folder } from "./folders";
import type { DownloadSnapshot } from "./types";

/** The toolbar heading for a view; a folder view takes the folder's label, a tag view the tag. */
export function viewTitle(filter: ViewFilter, folders: readonly Folder[]): string {
  if (filter.kind === "folder") {
    return folders.find((f) => f.dir === filter.dir)?.label ?? filter.dir;
  }
  if (filter.kind === "archived") return "Archived";
  if (filter.kind === "tag") return filter.archived ? `Archived › ${filter.tag}` : filter.tag;
  if (filter.status === "all") return "All downloads";
  return STATUS_FILTERS.find((f) => f.key === filter.status)!.label;
}

export interface EmptyCopy {
  title: string;
  body: string;
  /** Offer "Add download" (not for a name filter that matched nothing). */
  invitesAdd: boolean;
}

const EMPTY: Record<Extract<ViewFilter, { kind: "status" }>["status"], [string, string]> = {
  all: ["No downloads yet", "Paste a link anywhere with Ctrl+V, or use Add."],
  downloading: ["Nothing downloading", "Resume something paused or add a new link to get going."],
  waiting: [
    "Nothing queued",
    "Downloads wait here when the most allowed at once are already running.",
  ],
  paused: ["Nothing paused", "Pause a download to free its slot for something else."],
  done: ["Nothing completed yet", "Completed downloads stay here until you archive them."],
  failed: [
    "No failed downloads",
    "When a download fails, the reason and a Retry button show up here.",
  ],
  stopped: [
    "Nothing stopped",
    "Stopped downloads stay here. If you kept the partial file, you can resume them.",
  ],
};

/** What an empty table says, inviting the next step. */
export function emptyCopy(filter: ViewFilter, title: string, query: string): EmptyCopy {
  const q = query.trim();
  if (q) {
    return {
      title: `Nothing matches “${q}”`,
      body: "Try part of the filename or the site it came from.",
      invitesAdd: false,
    };
  }
  if (filter.kind === "folder") {
    return {
      title: `Nothing in ${title}`,
      body: "Downloads saved to this folder show up here.",
      invitesAdd: true,
    };
  }
  if (filter.kind === "archived") {
    return {
      title: "Nothing archived",
      body: "Archive finished downloads to keep the list short. They wait here until you bring them back or remove them.",
      invitesAdd: false,
    };
  }
  if (filter.kind === "tag") {
    return {
      title: filter.archived
        ? `No archived downloads tagged ${filter.tag}`
        : `Nothing tagged ${filter.tag}`,
      body: "Tag a download from its menu (Tags…) or its details.",
      invitesAdd: !filter.archived,
    };
  }
  const [t, body] = EMPTY[filter.status];
  return { title: t, body, invitesAdd: true };
}

/** The Stop dialog's "keep" choice hint. */
export function stopKeepHint(downloaded: number): string {
  return downloaded > 0
    ? `Keeps ${formatBytes(downloaded)} so Resume picks up where it left off.`
    : "Nothing downloaded yet, but its place and settings stay in the list.";
}

/** The Stop dialog's "delete" choice hint. */
export function stopDeleteHint(downloaded: number): string {
  return downloaded > 0
    ? `Frees ${formatBytes(downloaded)}. Starting again downloads from zero.`
    : "Starting again downloads from zero.";
}

/** The toast after a confirmed Stop. */
export function stoppedMessage(d: DownloadSnapshot, keep: boolean): string {
  if (!keep) return `Stopped ${d.filename} and deleted the partial file.`;
  return d.downloaded > 0
    ? `Stopped ${d.filename}. The partial file is kept.`
    : `Stopped ${d.filename}.`;
}

/** The toast after Remove. A finished file and a kept partial stay on disk; any other partial is discarded. */
export function removedMessage(d: DownloadSnapshot): string {
  if (d.status === "completed")
    return `Removed ${d.filename} from the list. The file stays on disk.`;
  if (d.status === "cancelled" && d.partial_kept) {
    return `Removed ${d.filename} from the list. Its partial file stays on disk.`;
  }
  return `Removed ${d.filename} from the list.`;
}

/**
 * What Remove does on disk to `ds`, as a sentence or two; null when it leaves
 * nothing. A finished file and a kept partial stay; a failed one's partial goes.
 */
export function removeDiskNote(ds: readonly DownloadSnapshot[]): string | null {
  const files = ds.filter((d) => d.status === "completed").length;
  const kept = ds.filter((d) => d.status === "cancelled" && d.partial_kept).length;
  const failed = ds.filter((d) => d.status === "failed").length;
  const notes: string[] = [];
  if (files && kept) notes.push("Files and kept partial files stay on disk.");
  else if (files) notes.push(files === 1 ? "The file stays on disk." : "The files stay on disk.");
  else if (kept) {
    notes.push(kept === 1 ? "The partial file stays on disk." : "The partial files stay on disk.");
  }
  if (failed) {
    notes.push(
      ds.length === 1
        ? "Its partial file is deleted."
        : failed === 1
          ? "The failed download's partial file is deleted."
          : "Failed downloads' partial files are deleted.",
    );
  }
  return notes.length ? notes.join(" ") : null;
}

/** Remove's tooltip on a row: what happens to its file. */
export function removeTitle(d: DownloadSnapshot): string {
  const note = removeDiskNote([d]);
  return note ? `Remove from rdm. ${note}` : "Remove from rdm.";
}

/** The Archived toolbar's Remove tooltip for the selected rows. */
export function removeSelectedTitle(ds: readonly DownloadSnapshot[]): string {
  const base =
    ds.length > 1
      ? "Remove the selected downloads from rdm for good."
      : "Remove the selected download from rdm for good.";
  const note = removeDiskNote(ds);
  return note ? `${base} ${note}` : base;
}

/** Archive's tooltip. */
export const archiveTitle = "Archive. It moves to Archived, out of the way, and can come back.";

/** The toast after Undo when none of the batch was still archived to bring back. */
export const nothingRestored = "Nothing to restore. Those downloads are no longer archived.";

/** The Remove dialog's heading. */
export function removeTitleFor(n: number): string {
  return n > 1 ? `Remove ${n} downloads?` : "Remove this download?";
}

/** The Remove dialog's text: what rdm forgets and what happens on disk. */
export function removeBody(ds: readonly DownloadSnapshot[]): string {
  const parts = [ds.length > 1 ? "rdm forgets them for good." : "rdm forgets it for good."];
  parts.push("Downloaded files stay on disk.");
  if (ds.some((d) => d.status === "cancelled" && d.partial_kept)) {
    parts.push("Kept partial files stay too.");
  }
  if (ds.some((d) => d.status === "failed")) parts.push("Unfinished partial files are deleted.");
  return parts.join(" ");
}

export function deletedPartialMessage(d: DownloadSnapshot): string {
  return `Deleted the partial file for ${d.filename}.`;
}

/** The toast after Retry all failed. */
export function retryAllMessage(failed: readonly DownloadSnapshot[]): string {
  return failed.length === 1
    ? `Retrying ${failed[0].filename}`
    : `Retrying ${failed.length} failed downloads`;
}

export function openFailedMessage(d: DownloadSnapshot, error: unknown): string {
  return `Couldn't open ${d.filename}. ${error}`;
}

/** The status bar's limit control. */
export function globalLimitLabel(limit: number | null): string {
  return limit == null ? "No speed limit" : `Limit ${formatSpeed(limit)}`;
}

/** The Stop dialog's heading. */
export function stopTitle(n: number): string {
  return n > 1 ? `Stop ${n} downloads?` : "Stop this download?";
}

/** The toast after stopping several downloads at once. */
export function stoppedManyMessage(n: number, keep: boolean): string {
  return keep
    ? `Stopped ${n} downloads.`
    : `Stopped ${n} downloads and deleted their partial files.`;
}

/** The toast after removing several downloads at once; finished files and kept partials stay. */
export function removedManyMessage(ds: readonly DownloadSnapshot[]): string {
  const stays = ds.some(
    (d) => d.status === "completed" || (d.status === "cancelled" && d.partial_kept),
  );
  return `Removed ${ds.length} downloads from the list.${stays ? " Files stay on disk." : ""}`;
}

/** The toast after deleting several kept partial files at once. */
export function deletedPartialsMessage(n: number): string {
  return `Deleted ${n} partial files.`;
}

/** The toast after a batch add (a single add needs none: its row appears). */
export function addedMessage(n: number): string {
  return `Added ${n} downloads.`;
}

/** The toast after Copy link. */
export function copiedMessage(n: number): string {
  return n === 1 ? "Copied the link." : `Copied ${n} links.`;
}

/** A waiting row's place in the queue, for its tooltip. */
export function queuePlace(index: number): string {
  if (index <= 0) return "Starts next";
  return `Queued behind ${index} other${index === 1 ? "" : "s"}`;
}
