// Start-at schedules as the UI words them: the typed time, the row and Coming up labels, the
// Reschedule menu's choices and the missed notice. The engine owns what a start time means (and
// holds the downloads); this only reads and shows it. Times are local "YYYY-MM-DDTHH:MM".

import { normalizeTime, timeLabel } from "./clock";
import { MONTHS } from "./format";
import type { DownloadSnapshot } from "./types";

const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const pad = (n: number) => String(n).padStart(2, "0");

/** A local moment as the engine reads it: "2026-03-09T22:00". */
export function isoOf(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** An engine time as a local Date. */
function dateOf(iso: string): Date {
  const [date, time] = iso.split("T");
  const [y, m, d] = date.split("-").map(Number);
  const [h, min] = time.split(":").map(Number);
  return new Date(y, m - 1, d, h, min);
}

const dayStart = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
const DAY_MS = 24 * 60 * 60 * 1000;

/**
 * What a typed start time means (`22:00`, `930`, `9:30 pm`): the next such time (tomorrow's if it has passed
 * today), like the engine reads it. Null if it isn't a time.
 */
export function startFromText(text: string, now: Date = new Date()): string | null {
  const t = normalizeTime(text);
  if (t === null) return null;
  const [h, m] = t.split(":").map(Number);
  const on = (days: number) =>
    new Date(now.getFullYear(), now.getMonth(), now.getDate() + days, h, m);
  return isoOf(isoOf(on(0)) >= isoOf(now) ? on(0) : on(1));
}

/** When it is, as it reads aloud: "today 22:00", "Thu 03:00", or "24 Nov 22:00" past a week. */
export function wording(iso: string, now: Date): string {
  const at = dateOf(iso);
  const days = Math.round((dayStart(at) - dayStart(now)) / DAY_MS);
  const time = timeLabel(iso.slice(11));
  if (days === 0) return `today ${time}`;
  if (days > 0 && days < 7) return `${DAYS[at.getDay()]} ${time}`;
  if (days < 0 && days > -7) return `${DAYS[at.getDay()]} ${time}`;
  return `${at.getDate()} ${MONTHS[at.getMonth()]} ${time}`;
}

/** Coming up: "Starts today 22:00" or "Starts Thu 03:00" (weekday and clock, never "tomorrow"). */
export const startsLabel = (iso: string, now: Date = new Date()) => `Starts ${wording(iso, now)}`;

/** A missed schedule: "Was due Mon 22:00". */
export const wasDueLabel = (iso: string, now: Date = new Date()) => `Was due ${wording(iso, now)}`;

/** A waiting row's state: "Scheduled for 01:00" (with its weekday when not today). */
export function scheduledForLabel(iso: string, now: Date = new Date()): string {
  const w = wording(iso, now);
  return `Scheduled for ${w.startsWith("today ") ? w.slice(6) : w}`;
}

/** A waiting row's state while a queue window holds it: "Queued until 22:00" (weekday when not today). */
export function queuedUntilLabel(iso: string, now: Date = new Date()): string {
  const w = wording(iso, now);
  return `Queued until ${w.startsWith("today ") ? w.slice(6) : w}`;
}

/** What a download's Progress cell says while it waits for a start time, else null. */
export function startState(d: DownloadSnapshot, now?: Date): string | null {
  if (d.start_at === null || d.status !== "queued") return null;
  return d.start_missed ? wasDueLabel(d.start_at, now) : scheduledForLabel(d.start_at, now);
}

/** What a download's Progress cell says while a closed queue window holds it, else null. */
export function windowState(d: DownloadSnapshot, now?: Date): string | null {
  if (d.window_until === null || d.status !== "queued" || d.start_at !== null) return null;
  return queuedUntilLabel(d.window_until, now);
}

/** Whether a row can be given a start time: waiting, paused, failed, or stopped with its partial kept. */
export const canSchedule = (d: DownloadSnapshot) =>
  !d.archived &&
  (d.status === "queued" ||
    d.status === "paused" ||
    d.status === "failed" ||
    (d.status === "cancelled" && d.partial_kept));

/** One start schedule: the downloads that share a start time. */
export interface StartGroup {
  group: number;
  /** "YYYY-MM-DDTHH:MM" */
  at: string;
  missed: boolean;
  ids: number[];
  /** The batch's name, else the file's, else "3 files". */
  name: string;
  /** "12 files" for a batch or several files; null for a lone file. */
  count: string | null;
}

/** The schedules the snapshots are in, soonest first. */
export function startGroups(ds: readonly DownloadSnapshot[]): StartGroup[] {
  const groups = new Map<number, DownloadSnapshot[]>();
  for (const d of ds) {
    if (d.start_at === null || d.start_group === null || d.status !== "queued") continue;
    groups.set(d.start_group, [...(groups.get(d.start_group) ?? []), d]);
  }
  return [...groups.entries()]
    .map(([group, rows]) => {
      const first = rows[0];
      const files = (n: number) => `${n} file${n === 1 ? "" : "s"}`;
      const label = rows.find((r) => r.batch_label)?.batch_label ?? null;
      return {
        group,
        at: first.start_at!,
        missed: first.start_missed,
        ids: rows.map((r) => r.id),
        name: label ?? (rows.length === 1 ? first.filename : files(rows.length)),
        count: rows.length === 1 && !label ? null : files(rows.length),
      };
    })
    .sort((a, b) => a.at.localeCompare(b.at) || a.group - b.group);
}

/** The missed notice's line: "3 scheduled starts were missed (50 downloads)". */
export function missedText(groups: readonly StartGroup[]): string {
  const downloads = groups.reduce((n, g) => n + g.ids.length, 0);
  const s = groups.length;
  return `${s} scheduled ${s === 1 ? "start was" : "starts were"} missed (${downloads} ${downloads === 1 ? "download" : "downloads"})`;
}

/** One entry of the Reschedule menu: the real time it lands on, and what to call it. */
export interface RescheduleChoice {
  /** "Tue 22:00" or "24 Nov 22:00" */
  when: string;
  /** "Tonight", "Tomorrow", "Next week" */
  name: string;
  at: string;
}

/** The evening a missed start is offered again: tonight (if it hasn't passed), tomorrow, next week. */
export function rescheduleChoices(now: Date = new Date(), evening = "22:00"): RescheduleChoice[] {
  const [h, m] = evening.split(":").map(Number);
  const at = (days: number) =>
    new Date(now.getFullYear(), now.getMonth(), now.getDate() + days, h, m);
  const out: RescheduleChoice[] = [];
  const choice = (days: number, name: string) => {
    const d = at(days);
    const when =
      days < 7
        ? `${DAYS[d.getDay()]} ${timeLabel(evening)}`
        : `${d.getDate()} ${MONTHS[d.getMonth()]} ${timeLabel(evening)}`;
    out.push({ when, name, at: isoOf(d) });
  };
  if (isoOf(at(0)) > isoOf(now)) choice(0, "Tonight");
  choice(1, "Tomorrow");
  choice(7, "Next week");
  return out;
}
