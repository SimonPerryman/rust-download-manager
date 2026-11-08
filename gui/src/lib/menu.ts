// The right-click menu's items for the rows it acts on, fitted to their
// statuses. Pure; App runs the chosen command.

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

/** What Remove from list leaves behind, said under it; null when nothing stays. */
function removeNote(list: readonly DownloadSnapshot[]): string | null {
  const removable = list.filter(isRemovable);
  const files = removable.filter((d) => d.status === "completed").length;
  const partials = removable.filter((d) => d.status === "cancelled" && d.partial_kept).length;
  if (files && partials) return "Files and kept partial files stay on disk.";
  if (files) return files === 1 ? "The file stays on disk." : "The files stay on disk.";
  if (partials)
    return partials === 1 ? "The partial file stays on disk." : "The partial files stay on disk.";
  return null;
}

/**
 * The menu for `list` (the selection, display order). `queue` is the waiting
 * order, for the move items; `failedCount` counts failed rows anywhere.
 */
export function menuItems(
  list: readonly DownloadSnapshot[],
  queue: readonly number[],
  failedCount: number,
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
    if (failedCount > 1) out.push(item("retryAll", "Retry all failed"));
  }
  sep();
  out.push(item("copyLink", list.length > 1 ? "Copy links" : "Copy link"));
  if (any(done)) out.push(item("addAgain", "Download again"));
  const kept = (d: DownloadSnapshot) => d.status === "cancelled" && d.partial_kept;
  if (any(kept) || any(isRemovable)) sep();
  if (any(kept)) {
    out.push(
      item(
        "deletePartial",
        list.filter(kept).length > 1 ? "Delete partial files" : "Delete partial file",
      ),
    );
  }
  if (any(isRemovable)) {
    out.push(item("remove", "Remove from list", "Del"));
    const note = removeNote(list);
    if (note) out.push({ kind: "note", text: note });
  }
  return out;
}

/** The contextual toolbar's actions for a selection of several rows, each only if it applies to one of them. */
export function selectionActions(list: readonly DownloadSnapshot[]): MenuCommand[] {
  const has = (f: (d: DownloadSnapshot) => boolean) => list.some(f);
  const out: MenuCommand[] = [];
  if (has((d) => d.status === "completed")) out.push("open");
  if (has(isPausable)) out.push("pause");
  if (toResume(list).length) out.push("resume");
  if (has(isStoppable)) out.push("stop");
  if (has((d) => d.status === "failed")) out.push("retry");
  if (has(isRemovable)) out.push("remove");
  return out;
}
