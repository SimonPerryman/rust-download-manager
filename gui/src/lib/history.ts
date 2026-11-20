import { timeLabel } from "./clock";
import type { HistoryEvent, HistoryKind, LogLine } from "./types";

/** The small type column's word for each kind of event. */
const LABEL: Record<HistoryKind, string> = {
  opened: "Opened",
  closed: "Closed",
  held: "Held",
  started: "Started",
  paused: "Paused",
  missed: "Missed",
  command: "Command",
  power: "Power",
  tag_added: "Tag",
  tag_removed: "Tag",
};

/** The word the type column shows for `kind`. */
export const kindLabel = (kind: HistoryKind): string => LABEL[kind];

/** An event's time of day in the OS's 12 or 24-hour style, e.g. "22:00". */
export function eventTime(at: number): string {
  const d = new Date(at);
  const pad = (n: number) => String(n).padStart(2, "0");
  return timeLabel(`${pad(d.getHours())}:${pad(d.getMinutes())}`);
}

/** A history event as a Log tab line, with its kind marking it. */
export function eventLine(e: HistoryEvent): LogLine {
  return { at: e.at, level: "info", message: e.details, fields: "", kind: e.kind };
}

/**
 * A download's engine lines with its history events among them by time. At the same instant a
 * line stays ahead of an event.
 */
export function mergeEvents(lines: readonly LogLine[], events: readonly HistoryEvent[]): LogLine[] {
  if (events.length === 0) return [...lines];
  const merged: LogLine[] = [];
  const rest = events.map(eventLine);
  let next = 0;
  for (const line of lines) {
    while (next < rest.length && rest[next].at < line.at) merged.push(rest[next++]);
    merged.push(line);
  }
  return merged.concat(rest.slice(next));
}

/**
 * Read a download's log: its engine lines and its history events as one list. A failed read of
 * the events leaves the lines as they are.
 */
export async function readLogWithEvents(
  id: number,
  readLines: (id: number) => Promise<LogLine[]>,
  readEvents: (id: number) => Promise<HistoryEvent[]>,
): Promise<LogLine[]> {
  const [lines, events] = await Promise.all([readLines(id), readEvents(id).catch(() => [])]);
  return mergeEvents(lines, events);
}

/** One day's events, newest first, under the day's heading. */
export interface DayGroup {
  /** "Today", "Yesterday", else "Wednesday 4 March". */
  heading: string;
  events: HistoryEvent[];
}

const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();

/** `events` grouped by local day, newest day first and newest event first within it. */
export function dayGroups(events: readonly HistoryEvent[], now: Date = new Date()): DayGroup[] {
  const today = startOfDay(now);
  const groups: DayGroup[] = [];
  for (const e of [...events].sort((a, b) => b.at - a.at)) {
    const day = new Date(e.at);
    const start = startOfDay(day);
    const days = Math.round((today - start) / 86_400_000);
    const heading =
      days === 0
        ? "Today"
        : days === 1
          ? "Yesterday"
          : day.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" });
    const last = groups[groups.length - 1];
    if (last && last.heading === heading) last.events.push(e);
    else groups.push({ heading, events: [e] });
  }
  return groups;
}
