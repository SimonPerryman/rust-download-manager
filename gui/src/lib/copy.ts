// The window's words: view titles, empty states, dialog hints and toast text.
// Sentence case, user-side verbs, and an action keeps its name through the flow
// ("Stop" leads to "Stopped", "Remove from list" to "Removed").

import { formatBytes, formatSpeed } from "./format";
import { STATUS_FILTERS, type ViewFilter } from "./filters";
import type { Folder } from "./folders";
import type { DownloadSnapshot } from "./types";

/** The toolbar heading for a view; a folder view takes the folder's label. */
export function viewTitle(filter: ViewFilter, folders: readonly Folder[]): string {
  if (filter.kind === "folder") {
    return folders.find((f) => f.dir === filter.dir)?.label ?? filter.dir;
  }
  if (filter.status === "all") return "All downloads";
  return STATUS_FILTERS.find((f) => f.key === filter.status)!.label;
}

export interface EmptyCopy {
  title: string;
  body: string;
  /** Offer "Add download" (not for a name filter that matched nothing). */
  invitesAdd: boolean;
}

const EMPTY: Record<Exclude<ViewFilter, { kind: "folder" }>["status"], [string, string]> = {
  all: ["No downloads yet", "Paste a link anywhere with Ctrl+V, or use Add."],
  downloading: ["Nothing downloading", "Resume something paused or add a new link to get going."],
  waiting: [
    "Nothing waiting",
    "Downloads wait here when the most allowed at once are already running.",
  ],
  paused: ["Nothing paused", "Pause a download to free its slot for something else."],
  done: ["Nothing finished yet", "Finished downloads stay here until you clear them."],
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

/** The toast after Remove from list. A finished file and a kept partial stay on disk; any other partial is discarded. */
export function removedMessage(d: DownloadSnapshot): string {
  if (d.status === "completed")
    return `Removed ${d.filename} from the list. The file stays on disk.`;
  if (d.status === "cancelled" && d.partial_kept) {
    return `Removed ${d.filename} from the list. Its partial file stays on disk.`;
  }
  return `Removed ${d.filename} from the list.`;
}

/** Remove from list's tooltip; a kept partial says it survives. */
export function removeTitle(d: DownloadSnapshot): string {
  return d.status === "cancelled" && d.partial_kept
    ? "Remove from list. The partial file stays on disk."
    : "Remove from list";
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
  return `Waiting behind ${index} other${index === 1 ? "" : "s"}`;
}
