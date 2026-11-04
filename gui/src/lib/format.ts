// Pure presentation helpers: turn the snapshot's raw numbers into the strings
// the mockup shows. This is formatting, not download logic.

import type { DownloadSnapshot } from "./types";

/** Human-readable byte size, e.g. `1.01 GB`, `8.6 MB`. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i += 1;
  }
  return `${value < 10 ? value.toFixed(2) : value.toFixed(1)} ${units[i]}`;
}

/** Throughput string, e.g. `11.4 MB/s`. */
export function formatSpeed(bytesPerSec: number): string {
  return `${formatBytes(Math.round(bytesPerSec))}/s`;
}

/** Coarse elapsed-time label for finished/ETA readouts, e.g. `3m 12s`. */
export function formatDuration(secs: number): string {
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  if (m < 60) return s ? `${m}m ${s}s` : `${m}m`;
  const h = Math.floor(m / 60);
  const rm = m % 60;
  return rm ? `${h}h ${rm}m` : `${h}h`;
}

/** "finished 2m ago" style relative time from a Unix-epoch-seconds stamp. */
export function formatAgo(epochSecs: number | null): string {
  if (epochSecs == null) return "";
  const delta = Math.max(0, Math.floor(Date.now() / 1000 - epochSecs));
  if (delta < 5) return "just now";
  return `${formatDuration(delta)} ago`;
}

/** Completion percentage 0–100, or null when the total is unknown. */
export function percent(d: DownloadSnapshot): number | null {
  if (!d.total || d.total <= 0) return null;
  return Math.min(100, Math.floor((d.downloaded / d.total) * 100));
}

/** "3 connections" while a download streams over more than one, else null. */
export function connectionsLabel(d: DownloadSnapshot): string | null {
  if (d.status !== "downloading" || d.connections <= 1) return null;
  return `${d.connections} connections`;
}

/** The URL's host, e.g. `releases.ubuntu.com`, for the source sub-line. */
export function host(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}
