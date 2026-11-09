import { afterEach, describe, expect, it, vi } from "vitest";
import {
  atBottom,
  isDetail,
  LOG_POLL_MS,
  logText,
  logTime,
  logView,
  mergeLog,
  pollLog,
  sameLog,
  STICK_PX,
  visibleLines,
} from "./log";
import type { LogLine } from "./types";

const line = (over: Partial<LogLine> = {}): LogLine => ({
  at: 1_000,
  level: "info",
  message: "Started · 4.00 MB · resumable · up to 4 lanes",
  fields: "",
  ...over,
});

describe("logTime", () => {
  it("shows the local time of day with seconds, zero-padded", () => {
    const at = new Date(2026, 10, 8, 9, 4, 5, 678).getTime();
    expect(logTime(at)).toBe("09:04:05");
    expect(logTime(new Date(2026, 10, 8, 23, 59, 59).getTime())).toBe("23:59:59");
  });
});

describe("logText", () => {
  it("puts the fields after the message", () => {
    expect(logText(line({ message: "Paused" }))).toBe("Paused");
    expect(logText(line({ message: "Target claimed", fields: "path=/data/a.iso" }))).toBe(
      "Target claimed path=/data/a.iso",
    );
  });
});

describe("visibleLines", () => {
  const lines = [
    line(),
    line({ level: "debug", message: "Lane 2 requested bytes 0–1023 · HTTP 206" }),
    line({ level: "warn", message: "Lane 2 lost connection — retrying (attempt 2)" }),
    line({ level: "trace", message: "ramp window" }),
    line({ level: "error", message: "Failed: server returned HTTP 403" }),
  ];

  it("hides per-request detail unless the Detailed view is on", () => {
    expect(visibleLines(lines, false).map((l) => l.level)).toEqual(["info", "warn", "error"]);
    expect(visibleLines(lines, true)).toBe(lines);
  });

  it("counts DEBUG and TRACE as detail", () => {
    expect(lines.map(isDetail)).toEqual([false, true, false, true, false]);
  });
});

describe("logView", () => {
  const request = line({ level: "debug", message: "Requested the file · HTTP 200 · 4.00 MB" });
  const view = (lines: LogLine[], detailed: boolean) =>
    logView(lines, visibleLines(lines, detailed));

  it("says nothing is logged when there are no lines", () => {
    expect(view([], false)).toBe("empty");
    expect(view([], true)).toBe("empty");
  });

  it("says only detail is logged while Detailed is off and nothing else is", () => {
    expect(view([request], false)).toBe("only-detail");
    expect(view([request], true)).toBe("list");
  });

  it("lists the lines once a concise one arrives", () => {
    expect(view([request, line()], false)).toBe("list");
  });
});

describe("sameLog", () => {
  it("is true only for the same lines in the same order", () => {
    const a = [line(), line({ at: 2_000, message: "Saved to /data/a.iso" })];
    expect(
      sameLog(
        a,
        a.map((l) => ({ ...l })),
      ),
    ).toBe(true);
    expect(sameLog([], [])).toBe(true);
    expect(sameLog(a, a.slice(1))).toBe(false);
    expect(sameLog(a, [a[0], { ...a[1], level: "error" }])).toBe(false);
    expect(sameLog(a, [a[0], { ...a[1], fields: "bytes=1" }])).toBe(false);
    expect(sameLog(a, [a[1], a[0]])).toBe(false);
  });
});

describe("atBottom", () => {
  it("follows new lines only while scrolled to (or just short of) the end", () => {
    const at = (scrollTop: number) =>
      atBottom({ scrollHeight: 1000, clientHeight: 200, scrollTop });
    expect(at(800)).toBe(true);
    expect(at(800 - STICK_PX)).toBe(true);
    expect(at(800 - STICK_PX - 1)).toBe(false);
    expect(at(0)).toBe(false);
    // Content shorter than the scroller is always at the end.
    expect(atBottom({ scrollHeight: 100, clientHeight: 200, scrollTop: 0 })).toBe(true);
  });
});

describe("mergeLog", () => {
  it("keeps what's shown on a failed read, and the same array when nothing changed", () => {
    const shown = [line()];
    expect(mergeLog(null, null)).toEqual([]);
    expect(mergeLog(shown, null)).toBe(shown);
    expect(mergeLog(shown, [line()])).toBe(shown);
    const next = [line(), line({ at: 2_000 })];
    expect(mergeLog(shown, next)).toBe(next);
    expect(mergeLog(null, next)).toBe(next);
  });
});

describe("pollLog", () => {
  afterEach(() => vi.useRealTimers());

  it("reads now and every interval, and never again once stopped", async () => {
    vi.useFakeTimers();
    const read = vi.fn(() => Promise.resolve([line()]));
    const got: (LogLine[] | null)[] = [];
    const stop = pollLog(3, read, (l) => got.push(l));
    await vi.advanceTimersByTimeAsync(LOG_POLL_MS * 2);
    expect(read.mock.calls).toEqual([[3], [3], [3]]);
    expect(got).toHaveLength(3);

    stop();
    await vi.advanceTimersByTimeAsync(LOG_POLL_MS * 3);
    expect(read).toHaveBeenCalledTimes(3);
  });

  it("drops a reply that lands after stop, and reports a failed read as null", async () => {
    let settle: (l: LogLine[]) => void = () => {};
    const got: (LogLine[] | null)[] = [];
    const stop = pollLog(
      1,
      () => new Promise((r) => (settle = r)),
      (l) => got.push(l),
    );
    stop();
    settle([line()]);
    await Promise.resolve();
    expect(got).toEqual([]);

    const stopFailing = pollLog(
      1,
      () => Promise.reject(new Error("gone")),
      (l) => got.push(l),
    );
    await vi.waitFor(() => expect(got).toEqual([null]));
    stopFailing();
  });
});
