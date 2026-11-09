import type { LogLine } from "./types";

/** How often the open Log tab asks the engine for new lines. */
export const LOG_POLL_MS = 1000;

/** How close to the bottom (px) still counts as following new lines. */
export const STICK_PX = 8;

/** Whether a scroller sits at (or within `STICK_PX` of) its bottom, so new lines should keep it there. */
export function atBottom(el: { scrollHeight: number; scrollTop: number; clientHeight: number }) {
  return el.scrollHeight - el.scrollTop - el.clientHeight <= STICK_PX;
}

/** A log line's time of day in local time, e.g. "09:04:05". */
export function logTime(at: number): string {
  const d = new Date(at);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/** A log line's text: the message, then its fields. */
export function logText(line: LogLine): string {
  return line.fields ? `${line.message} ${line.fields}` : line.message;
}

/** Whether a line is per-request detail (DEBUG and below), shown only in the Detailed view. */
export function isDetail(line: LogLine): boolean {
  return line.level === "debug" || line.level === "trace";
}

/** The lines the Log tab shows: all of them when `detailed`, else the concise ones. */
export function visibleLines(lines: readonly LogLine[], detailed: boolean): readonly LogLine[] {
  return detailed ? lines : lines.filter((l) => !isDetail(l));
}

/**
 * What the Log tab shows for `lines` it has read and the `shown` subset:
 * "empty" when nothing is logged, "only-detail" when every line is hidden
 * detail (DEBUG lines come before "Started"), else the "list".
 */
export function logView(
  lines: readonly LogLine[],
  shown: readonly LogLine[],
): "empty" | "only-detail" | "list" {
  if (lines.length === 0) return "empty";
  return shown.length === 0 ? "only-detail" : "list";
}

/** Whether two reads of a log hold the same lines, so an unchanged poll doesn't re-render. */
export function sameLog(a: readonly LogLine[], b: readonly LogLine[]): boolean {
  return (
    a.length === b.length &&
    a.every(
      (l, i) =>
        l.at === b[i].at &&
        l.level === b[i].level &&
        l.message === b[i].message &&
        l.fields === b[i].fields,
    )
  );
}

/**
 * What the Log tab shows after a read: a failed read (`null`) keeps what's
 * shown, or shows no lines if nothing has arrived yet; an unchanged read keeps
 * the same array, so it doesn't re-render.
 */
export function mergeLog(prev: LogLine[] | null, next: LogLine[] | null): LogLine[] {
  if (next == null) return prev ?? [];
  return prev && sameLog(prev, next) ? prev : next;
}

/**
 * Read `id`'s log now and every `LOG_POLL_MS` until the returned stop function
 * runs, passing each read to `onLines` (`null` for a failed one). Nothing is
 * delivered after stop, so a slow reply for a closed tab is dropped.
 */
export function pollLog(
  id: number,
  read: (id: number) => Promise<LogLine[]>,
  onLines: (lines: LogLine[] | null) => void,
): () => void {
  let live = true;
  const tick = () =>
    read(id).then(
      (lines) => live && onLines(lines),
      () => live && onLines(null),
    );
  void tick();
  const timer = setInterval(tick, LOG_POLL_MS);
  return () => {
    live = false;
    clearInterval(timer);
  };
}
