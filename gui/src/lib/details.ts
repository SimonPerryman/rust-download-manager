// What the details pane says about one download. Pure presentation over the
// snapshot; the engine decides every fact.

import { formatBytes, formatSpeed, MONTHS } from "./format";
import { precisePercent } from "./rowView";
import type { DownloadSnapshot } from "./types";

/** How long a download ran, e.g. "under 1 s", "48 s", "2 min 14 s", "1 h 5 min". */
export function formatElapsed(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 1) return "under 1 s";
  if (s < 60) return `${s} s`;
  if (s < 3600) {
    const sec = s % 60;
    return sec ? `${Math.floor(s / 60)} min ${sec} s` : `${Math.floor(s / 60)} min`;
  }
  const m = Math.floor((s % 3600) / 60);
  return m ? `${Math.floor(s / 3600)} h ${m} min` : `${Math.floor(s / 3600)} h`;
}

const CHECKSUM_WORDS = {
  verified: "checksum verified",
  pending: "checking the checksum",
  mismatch: "checksum doesn't match",
  none: "no checksum given",
} as const;

/**
 * A finished download's one-line summary: "5.8 GB" then "in 2 min 14 s, average
 * 44.0 MB/s, checksum verified". Time and average need the engine's active time,
 * so one whose time is unknown says only its size and checksum.
 */
export function summaryParts(d: DownloadSnapshot): { size: string; rest: string } {
  const bytes = d.total ?? d.downloaded;
  const parts: string[] = [];
  if (d.active_ms != null) {
    parts.push(`in ${formatElapsed(d.active_ms)}`);
    parts.push(`average ${formatSpeed(bytes / (Math.max(1, d.active_ms) / 1000))}`);
  }
  parts.push(CHECKSUM_WORDS[d.checksum_state]);
  return { size: formatBytes(bytes), rest: parts.join(", ") };
}

/** The short state next to the pane's title, e.g. "42.1% of 5.80 GB". */
export function statePhrase(d: DownloadSnapshot): string {
  switch (d.status) {
    case "downloading": {
      const pct = precisePercent(d);
      if (pct == null || d.total == null) return `${formatBytes(d.downloaded)} so far`;
      return `${pct.toFixed(1)}% of ${formatBytes(d.total)}`;
    }
    case "queued":
      return "Queued for a free slot";
    case "paused":
      return "Paused";
    case "completed":
      return "Completed";
    case "failed":
      return "Failed";
    case "cancelled":
      return d.partial_kept ? "Stopped, partial file kept" : "Stopped";
  }
}

export interface ChecksumView {
  text: string;
  tone: "ok" | "checking" | "bad" | "plain";
  /** The expected hash (hex), shown under the text in mono. */
  hash: string | null;
  /** Offer "Verify checksum": the file is done and there's no good result yet. */
  canVerify: boolean;
  hint: string | null;
}

const hexOf = (checksum: string | null) => checksum?.replace(/^sha256:/i, "") ?? null;

export function checksumView(d: DownloadSnapshot): ChecksumView {
  const done = d.status === "completed";
  const hash = hexOf(d.checksum);
  switch (d.checksum_state) {
    case "pending":
      return { text: "Checking SHA-256", tone: "checking", hash, canVerify: false, hint: null };
    case "verified":
      return { text: "SHA-256 verified", tone: "ok", hash, canVerify: false, hint: null };
    case "mismatch":
      return {
        text: "SHA-256 doesn't match",
        tone: "bad",
        hash,
        canVerify: done,
        hint: done ? null : "The download will start over when retried.",
      };
    case "none":
      if (hash && done)
        return { text: "Not checked yet", tone: "plain", hash, canVerify: true, hint: null };
      if (hash)
        return {
          text: "Checks SHA-256 when done",
          tone: "plain",
          hash,
          canVerify: false,
          hint: null,
        };
      return {
        text: "No checksum given",
        tone: "plain",
        hash: null,
        canVerify: done,
        hint: done ? null : "You can verify one once it's done.",
      };
  }
}

/** The limit that actually caps this download: the lower of its own and the global one. */
export function effectiveLimit(own: number | null, global: number | null): number | null {
  if (own == null) return global;
  if (global == null) return own;
  return Math.min(own, global);
}

export const PANE_DEFAULT_H = 264;
export const PANE_MIN_H = 160;
/** Room the toolbar, the table header and a few rows keep above the pane. */
const PANE_KEEP_ABOVE = 200;

/** A pane height that leaves the table usable in a `viewport`-tall window. */
export function clampPaneHeight(h: number, viewport: number): number {
  const max = Math.max(PANE_MIN_H, viewport - PANE_KEEP_ABOVE);
  return Math.round(Math.min(max, Math.max(PANE_MIN_H, h)));
}

/** A pasted SHA-256: 64 hex digits, optionally prefixed `sha256:`. */
export function isSha256(input: string): boolean {
  return /^(sha256:)?[0-9a-f]{64}$/i.test(input.trim());
}

/** What the Verify form says when a hash can't be checked. */
export const NOT_A_HASH =
  "That isn't a SHA-256 hash. Paste the 64 letters and digits the site lists.";

/** A rejected `verify_checksum` in plain words (the engine's reason, tidied). */
export function verifyErrorText(error: unknown): string {
  const raw = String(error)
    .replace(/^Error:\s*/, "")
    .replace(/^invalid request:\s*/i, "")
    .trim();
  if (!raw) return "Couldn't verify the file.";
  const reason = raw[0].toUpperCase() + raw.slice(1);
  return `Couldn't verify the file. ${reason}${/[.!?]$/.test(reason) ? "" : "."}`;
}

const pad = (n: number) => String(n).padStart(2, "0");

/** When something happened, in local time: "today at 14:05", "yesterday at 09:12", "3 Nov at 14:05". */
export function formatWhen(epochSecs: number, now: Date = new Date()): string {
  const at = new Date(epochSecs * 1000);
  const time = `${pad(at.getHours())}:${pad(at.getMinutes())}`;
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1).getTime();
  if (at.getTime() >= today) return `today at ${time}`;
  if (at.getTime() >= yesterday) return `yesterday at ${time}`;
  return `${at.getDate()} ${MONTHS[at.getMonth()]} at ${time}`;
}

const ENDED = { completed: "finished", failed: "failed", cancelled: "stopped" } as const;

/** The pane's dates line, e.g. "Added today at 14:05, finished today at 14:07". */
export function datesLine(d: DownloadSnapshot, now: Date = new Date()): string {
  const parts: string[] = [];
  if (d.added_at != null) parts.push(`Added ${formatWhen(d.added_at, now)}`);
  const ended = d.status in ENDED ? ENDED[d.status as keyof typeof ENDED] : null;
  if (ended && d.finished_at != null) parts.push(`${ended} ${formatWhen(d.finished_at, now)}`);
  const line = parts.join(", ");
  return line && line[0].toUpperCase() + line.slice(1);
}
