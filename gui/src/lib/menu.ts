// The right-click menu's items for the rows it acts on, fitted to their
// statuses. Pure; useTableInput runs the chosen command.

import { isArchivable, toRemove } from "./archive";
import { removeDiskNote } from "./copy";
import { isPausable, isRemovable, isStoppable, toResume } from "./rowView";
import type { DownloadSnapshot } from "./types";

export type MenuCommand =
  | "open"
  | "reveal"
  | "pause"
  | "resume"
  | "startNext"
  | "moveUp"
  | "moveDown"
  | "stop"
  | "retry"
  | "retryAll"
  | "copyLink"
  | "addAgain"
  | "deletePartial"
  | "tags"
  | "archive"
  | "unarchive"
  | "remove";

export type MenuEntry =
  | { kind: "item"; cmd: MenuCommand; label: string; shortcut?: string }
  | { kind: "separator" }
  /** Quiet text under the item before it. */
  | { kind: "note"; text: string };

const item = (cmd: MenuCommand, label: string, shortcut?: string): MenuEntry => ({
  kind: "item",
  cmd,
  label,
  shortcut,
});

/**
 * The menu for `list` (the selection, display order). `queue` is the waiting
 * order, for the move items; `failedCount` counts failed rows outside the
 * archive. In the Archived view (`archivedView`) Del and the last items
 * unarchive and remove; elsewhere they archive.
 */
export function menuItems(
  list: readonly DownloadSnapshot[],
  queue: readonly number[],
  failedCount: number,
  archivedView = false,
): MenuEntry[] {
  const out: MenuEntry[] = [];
  const sep = () => {
    if (out.length && out[out.length - 1].kind !== "separator") out.push({ kind: "separator" });
  };
  const one = list.length === 1 ? list[0] : null;
  const any = (f: (d: DownloadSnapshot) => boolean) => list.some(f);
  const done = (d: DownloadSnapshot) => d.status === "completed";
  const failed = (d: DownloadSnapshot) => d.status === "failed";

  if (any(done)) {
    out.push(item("open", "Open", one ? "Enter" : undefined));
    if (one) out.push(item("reveal", "Show in folder"));
  }
  if (any(isPausable)) out.push(item("pause", "Pause", "Space"));
  if (toResume(list).length) {
    const again = one?.status === "cancelled" && !one.partial_kept;
    out.push(item("resume", again ? "Download again" : "Resume", again ? undefined : "Space"));
  }
  if (one?.status === "queued") {
    const i = queue.indexOf(one.id);
    if (i > 0) {
      out.push(item("startNext", "Start next"));
      out.push(item("moveUp", "Move up", "Alt+↑"));
    }
    if (i >= 0 && i < queue.length - 1) out.push(item("moveDown", "Move down", "Alt+↓"));
  }
  if (any(isStoppable)) out.push(item("stop", "Stop…"));
  if (any(failed)) {
    out.push(item("retry", "Retry"));
    // Retry all failed acts on the working views, so the archive doesn't offer it.
    if (failedCount > 1 && !archivedView) out.push(item("retryAll", "Retry all failed"));
  }
  sep();
  out.push(item("copyLink", list.length > 1 ? "Copy links" : "Copy link"));
  if (any(done)) out.push(item("addAgain", "Download again"));
  out.push(item("tags", "Tags…"));
  const kept = (d: DownloadSnapshot) => d.status === "cancelled" && d.partial_kept;
  sep();
  if (any(kept)) {
    out.push(
      item(
        "deletePartial",
        list.filter(kept).length > 1 ? "Delete partial files" : "Delete partial file",
      ),
    );
  }
  if (archivedView) {
    out.push(item("unarchive", "Unarchive"));
    if (any(isRemovable)) {
      out.push(item("remove", "Remove", "Del"));
      const note = removeDiskNote(toRemove(list));
      if (note) out.push({ kind: "note", text: note });
    }
  } else if (any(isArchivable)) {
    out.push(item("archive", "Archive", "Del"));
  }
  if (out[out.length - 1].kind === "separator") out.pop();
  return out;
}

/**
 * The contextual toolbar's actions for a selection of several rows, each only
 * if it applies to one of them; in the Archived view, Unarchive and Remove.
 */
export function selectionActions(
  list: readonly DownloadSnapshot[],
  archivedView = false,
): MenuCommand[] {
  const has = (f: (d: DownloadSnapshot) => boolean) => list.some(f);
  const out: MenuCommand[] = [];
  if (has((d) => d.status === "completed")) out.push("open");
  if (has(isPausable)) out.push("pause");
  if (toResume(list).length) out.push("resume");
  if (has(isStoppable)) out.push("stop");
  if (has((d) => d.status === "failed")) out.push("retry");
  out.push("tags");
  if (archivedView) {
    out.push("unarchive");
    if (has(isRemovable)) out.push("remove");
  } else if (has(isArchivable)) {
    out.push("archive");
  }
  return out;
}
