// What each table row shows for its snapshot: the progress cell, the size and
// the actions. Pure presentation; the engine's status decides everything.

import { formatBytes } from "./format";
import type { DownloadSnapshot } from "./types";

/** The Progress cell: a bar with its percentage, or a quiet word for rows that aren't moving. */
export type ProgressView =
  | { kind: "bar"; pct: number; text: string; muted: boolean }
  | { kind: "text"; text: string; tone: "waiting" | "done" | "failed" | "stopped" | "plain" };

/** Percent done to one decimal place, rounded down so 99.96 % never reads "100.0%"; null when unknown. */
export function precisePercent(d: DownloadSnapshot): number | null {
  if (!d.total || d.total <= 0) return null;
  return Math.min(100, Math.floor((d.downloaded / d.total) * 1000) / 10);
}

export function progressView(d: DownloadSnapshot): ProgressView {
  const pct = precisePercent(d);
  switch (d.status) {
    case "downloading":
      // No length from the server: the bytes so far are the only honest progress.
      if (pct == null) return { kind: "text", text: formatBytes(d.downloaded), tone: "plain" };
      return { kind: "bar", pct, text: `${pct.toFixed(1)}%`, muted: false };
    case "queued":
      return { kind: "text", text: "Waiting", tone: "waiting" };
    case "paused":
      if (pct == null) return { kind: "text", text: "Paused", tone: "stopped" };
      return { kind: "bar", pct, text: `${Math.floor(pct)}%`, muted: true };
    case "completed":
      return { kind: "text", text: "Done", tone: "done" };
    case "failed":
      return { kind: "text", text: "Failed", tone: "failed" };
    case "cancelled":
      if (!d.partial_kept || pct == null) return { kind: "text", text: "Stopped", tone: "stopped" };
      return { kind: "bar", pct, text: `${Math.floor(pct)}%`, muted: true };
  }
}

/** The Size column: the file's size, blank until the server says. */
export function sizeText(d: DownloadSnapshot): string {
  return d.total != null ? formatBytes(d.total) : "";
}

export type RowActionKey =
  | "limit"
  | "pause"
  | "resume"
  | "stop"
  | "open"
  | "reveal"
  | "remove"
  | "archive"
  | "retry"
  | "deletePartial"
  | "downloadAgain";

export interface RowAction {
  key: RowActionKey;
  /** The accessible name; primary and soft buttons also show it as text, icon buttons don't. */
  label: string;
  style: "icon" | "primary" | "soft";
}

const icon = (key: RowActionKey, label: string): RowAction => ({ key, label, style: "icon" });
const LIMIT = icon("limit", "Speed limit");
const PAUSE = icon("pause", "Pause");
const STOP = icon("stop", "Stop");
const REVEAL = icon("reveal", "Show in folder");
const REMOVE = icon("remove", "Remove");
const ARCHIVE = icon("archive", "Archive");

/**
 * The row's actions, left to right. `lingering` = finished this session and not opened yet.
 * A finished row's last button archives it, or in the archive removes it.
 */
export function rowActions(d: DownloadSnapshot, lingering: boolean): RowAction[] {
  const lastAction = d.archived ? REMOVE : ARCHIVE;
  switch (d.status) {
    case "downloading":
    case "queued":
      return [LIMIT, PAUSE, STOP];
    case "paused":
      return [LIMIT, icon("resume", "Resume"), STOP];
    case "completed":
      return [
        lingering ? { key: "open", label: "Open", style: "primary" } : icon("open", "Open"),
        REVEAL,
        lastAction,
      ];
    case "failed":
      return [{ key: "retry", label: "Retry", style: "soft" }, lastAction];
    case "cancelled":
      return d.partial_kept
        ? [
            { key: "resume", label: "Resume", style: "soft" },
            icon("deletePartial", "Delete partial file"),
            lastAction,
          ]
        : [{ key: "downloadAgain", label: "Download again", style: "soft" }, lastAction];
  }
}

/** The toolbar's bulk control: Pause all while anything runs or waits, else Resume all while anything is paused. */
export function pauseAllMode(ds: DownloadSnapshot[]): "pause" | "resume" | null {
  if (ds.some((d) => d.status === "downloading" || d.status === "queued")) return "pause";
  if (ds.some((d) => d.status === "paused")) return "resume";
  return null;
}

/** Combined speed of everything downloading, in bytes/s. */
export function totalSpeed(ds: DownloadSnapshot[]): number {
  return ds.reduce((sum, d) => sum + (d.status === "downloading" ? d.bytes_per_sec : 0), 0);
}

/** Whether a Stop still applies (the dialog closes itself once it doesn't). */
export function isStoppable(d: DownloadSnapshot | undefined): d is DownloadSnapshot {
  return (
    d != null && (d.status === "downloading" || d.status === "queued" || d.status === "paused")
  );
}

/** Running or waiting: Pause applies. */
export function isPausable(d: DownloadSnapshot): boolean {
  return d.status === "downloading" || d.status === "queued";
}

/** Paused or stopped: the engine's `resume` continues it (or, with no partial kept, starts it again). */
export function isResumable(d: DownloadSnapshot): boolean {
  return d.status === "paused" || d.status === "cancelled";
}

/**
 * The rows a Resume on `list` acts on. One row: any paused or stopped one (a
 * stopped row with no partial starts again, which its label says). Several:
 * only what continues where it left off, so a bulk Resume never quietly
 * restarts a download from zero.
 */
export function toResume(list: readonly DownloadSnapshot[]): DownloadSnapshot[] {
  if (list.length === 1) return list.filter(isResumable);
  return list.filter(continues);
}

/** Paused, or stopped with its partial kept: a Resume picks up where it left off. */
export function continues(d: DownloadSnapshot): boolean {
  return d.status === "paused" || (d.status === "cancelled" && d.partial_kept);
}

/** Finished one way or another: Remove applies (from the Archived view). */
export function isRemovable(d: DownloadSnapshot): boolean {
  return d.status === "completed" || d.status === "failed" || d.status === "cancelled";
}
