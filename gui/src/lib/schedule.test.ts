import { describe, expect, it } from "vitest";
import {
  autoName,
  draftOf,
  draftSummary,
  draftProblem,
  limitBytes,
  NEW_SPEED,
  NEW_WINDOW,
  pausedNote,
  ruleName,
  ruleOf,
  ruleSummary,
  speedOf,
  statusLine,
  summary,
  validTime,
  whenLabel,
  withCopy,
} from "./schedule";
import type { QueueState, Schedule } from "./types";

const MB = 1024 * 1024;

const state = (
  open: boolean,
  until: string | null,
  speed: Partial<QueueState> = {},
): QueueState => ({
  open,
  until,
  limit: null,
  limit_until: null,
  limit_then: null,
  ...speed,
});

// 2026-11-17 is a Tuesday.
const NOW = new Date(2026, 10, 17, 12, 0);

describe("scoped rules", () => {
  const tagged = { ...NEW_WINDOW, scopeKind: "tag" as const, scopeText: "course-2" };

  it("words who a rule applies to in its sentence", () => {
    expect(draftSummary(tagged)).toBe(
      "Downloads tagged course-2 run from 22:00 to 08:00 every day, and pause at 08:00.",
    );
    expect(draftSummary({ ...tagged, finishRunning: true })).toBe(
      "New downloads tagged course-2 start from 22:00 to 08:00 every day. Running ones finish.",
    );
    expect(draftSummary({ ...NEW_SPEED, scopeKind: "host", scopeText: "Example.com" })).toBe(
      "Downloads from example.com are limited to 5 MB/s from 08:00 to 18:00 on weekdays.",
    );
    expect(
      draftSummary({ ...tagged, scopeKind: "batch", scopeBatch: { id: 4, label: "Course" } }),
    ).toBe("Downloads in batch Course run from 22:00 to 08:00 every day, and pause at 08:00.");
    expect(draftSummary(NEW_WINDOW)).toBe(
      "Downloads run from 22:00 to 08:00 every day, and pause at 08:00.",
    );
  });

  it("saves a scope only when it isn't all downloads, and reads it back", () => {
    expect(ruleOf(NEW_WINDOW)).not.toHaveProperty("scope");
    const rule = ruleOf(tagged);
    expect(rule.scope).toEqual({ kind: "tag", tag: "course-2" });
    expect(draftOf(rule)).toMatchObject({ scopeKind: "tag", scopeText: "course-2" });
    const speed = speedOf({ ...NEW_SPEED, scopeKind: "host", scopeText: " CDN.example.com " });
    expect(speed.scope).toEqual({ kind: "host", host: "cdn.example.com" });
    expect(draftOf(speed)).toMatchObject({ scopeKind: "host", scopeText: "cdn.example.com" });
    expect(draftOf({ ...rule, scope: undefined })).toMatchObject({ scopeKind: "all" });
  });

  it("needs a value for a tag, a host or a batch", () => {
    expect(draftProblem({ ...tagged, scopeText: " " })).toBe("Enter a tag");
    expect(draftProblem({ ...tagged, scopeKind: "host", scopeText: "" })).toBe("Enter a host");
    expect(draftProblem({ ...tagged, scopeKind: "batch", scopeBatch: null })).toBe("Pick a batch");
    expect(draftProblem(tagged)).toBeNull();
  });

  it("shows the scope on a speed row and keeps the status line truthful", () => {
    const speed = speedOf({ ...NEW_SPEED, scopeKind: "tag", scopeText: "x" });
    expect(ruleSummary(speed)).toBe("5 MB/s, 08:00–18:00, weekdays, tagged x");
    const only: Schedule = { queue_windows: [ruleOf(tagged)], speed_entries: [] };
    expect(statusLine(only, state(true, null))).toBe(
      "Downloads run any time, except those a rule names.",
    );
    const mixed: Schedule = {
      queue_windows: [ruleOf(NEW_WINDOW), ruleOf(tagged)],
      speed_entries: [],
    };
    expect(statusLine(mixed, state(true, "2026-11-17T22:00"), NOW)).toContain(
      "Other rules name some downloads.",
    );
  });
});

describe("summary", () => {
  it("says when downloads run and pause", () => {
    expect(summary(NEW_WINDOW)).toBe(
      "Downloads run from 22:00 to 08:00 every day, and pause at 08:00.",
    );
  });

  it("says running downloads finish", () => {
    expect(summary({ ...NEW_WINDOW, finishRunning: true, days: ["sat", "sun"] })).toBe(
      "New downloads start from 22:00 to 08:00 on weekends. Running ones finish.",
    );
  });
});

describe("draftProblem", () => {
  it("accepts the default and refuses an empty set of days", () => {
    expect(draftProblem(NEW_WINDOW)).toBeNull();
    expect(draftProblem({ ...NEW_WINDOW, days: [] })).toBe("Pick at least one day");
  });

  it("refuses bad or equal times", () => {
    expect(draftProblem({ ...NEW_WINDOW, start: "8:00" })).toBe("Times look like 22:00");
    expect(draftProblem({ ...NEW_WINDOW, end: "22:00" })).toMatch(/same time/);
    expect(validTime("24:00")).toBe(false);
    expect(validTime("23:59")).toBe(true);
  });
});

describe("names", () => {
  it("makes one from the times when none was given", () => {
    expect(autoName("22:00", "06:00", NEW_WINDOW.days)).toBe("Overnight only");
    expect(autoName("08:00", "18:00", ["mon", "tue", "wed", "thu", "fri"])).toBe(
      "08:00 to 18:00, Weekdays",
    );
    expect(ruleName(ruleOf({ ...NEW_WINDOW, name: " Nights " }))).toBe("Nights");
    expect(ruleOf(NEW_WINDOW)).not.toHaveProperty("name");
  });
});

describe("queue state wording", () => {
  it("leaves out the day when it is today", () => {
    expect(whenLabel("2026-11-17T22:00", NOW)).toBe("22:00");
    expect(whenLabel("2026-11-18T08:00", NOW)).toBe("Wed 08:00");
  });

  it("notes a closed queue only", () => {
    expect(pausedNote(state(false, "2026-11-17T22:00"), NOW)).toBe("Queue paused until 22:00");
    expect(pausedNote(state(true, "2026-11-17T22:00"), NOW)).toBeNull();
    expect(pausedNote(null, NOW)).toBeNull();
  });

  it("states the queue in one line", () => {
    const none: Schedule = { queue_windows: [], speed_entries: [] };
    expect(statusLine(none, state(true, null), NOW)).toBe("No schedules. Downloads run any time.");
    const on = { ...none, queue_windows: [ruleOf(NEW_WINDOW)] };
    expect(statusLine(on, state(false, "2026-11-17T22:00"), NOW)).toBe(
      "Queue paused. The window opens at 22:00.",
    );
    expect(statusLine(on, state(true, "2026-11-18T08:00"), NOW)).toBe(
      "Queue open. The window closes at Wed 08:00.",
    );
    on.queue_windows[0].enabled = false;
    expect(statusLine(on, state(true, null), NOW)).toMatch(/^All schedules are off/);
  });
});

describe("copy", () => {
  it("puts a copy right after the rule", () => {
    const s: Schedule = {
      queue_windows: [ruleOf(NEW_WINDOW), ruleOf({ ...NEW_WINDOW, start: "01:00" })],
      speed_entries: [],
    };
    const next = withCopy(s, { kind: "window", index: 0 });
    expect(next.queue_windows).toHaveLength(3);
    expect(next.queue_windows[1]).toEqual(next.queue_windows[0]);
    expect(next.queue_windows[1]).not.toBe(next.queue_windows[0]);
  });
});

describe("speed limit rules", () => {
  const cap = { ...NEW_SPEED, limit: "5" };

  it("says what the limit does, in the footer sentence", () => {
    expect(summary(cap)).toBe("Downloads are limited to 5 MB/s from 08:00 to 18:00 on weekdays.");
    expect(summary({ ...cap, days: ["mon", "tue", "thu"] })).toBe(
      "Downloads are limited to 5 MB/s from 08:00 to 18:00 on Mon, Tue and Thu.",
    );
    expect(summary({ ...cap, limit: "0.5" })).toContain("limited to 512 KB/s from");
  });

  it("needs a speed above zero", () => {
    expect(draftProblem(cap)).toBeNull();
    for (const bad of ["", "0", "0.0", "abc", "."]) {
      expect(draftProblem({ ...cap, limit: bad }), bad).toBe("Enter a speed above 0");
    }
    expect(limitBytes("1.5")).toBe(1.5 * MB);
    expect(limitBytes(".5")).toBe(0.5 * MB);
  });

  it("makes the entry the engine reads, and lists it as the mock-up words it", () => {
    const entry = speedOf(cap);
    expect(entry).toEqual({
      enabled: true,
      window: { start: "08:00", end: "18:00", days: ["mon", "tue", "wed", "thu", "fri"] },
      limit: 5 * MB,
    });
    expect(ruleName(entry)).toBe("5 MB/s limit");
    expect(ruleName(speedOf({ ...cap, name: " Daytime cap " }))).toBe("Daytime cap");
    expect(ruleSummary(entry)).toBe("5 MB/s, 08:00\u201318:00, weekdays");
    expect(ruleSummary(speedOf({ ...cap, days: ["mon", "tue", "thu"] }))).toBe(
      "5 MB/s, 08:00\u201318:00, Mon, Tue and Thu",
    );
  });

  it("puts a copy after its own kind only", () => {
    const s: Schedule = {
      queue_windows: [ruleOf(NEW_WINDOW)],
      speed_entries: [speedOf(cap), speedOf({ ...cap, limit: "1" })],
    };
    const next = withCopy(s, { kind: "speed", index: 0 });
    expect(next.queue_windows).toHaveLength(1);
    expect(next.speed_entries.map((e) => e.limit)).toEqual([5 * MB, 5 * MB, MB]);
  });

  it("extends the status line with the cap in force or the next one", () => {
    const s: Schedule = { queue_windows: [], speed_entries: [speedOf(cap)] };
    expect(
      statusLine(s, state(true, null, { limit: 5 * MB, limit_until: "2026-11-17T18:00" }), NOW),
    ).toBe("Downloads run any time. A 5 MB/s cap applies until 18:00.");
    expect(
      statusLine(
        s,
        state(true, null, { limit_until: "2026-11-18T08:00", limit_then: 5 * MB }),
        NOW,
      ),
    ).toBe("Downloads run any time. A 5 MB/s cap starts at Wed 08:00.");
    const both = { ...s, queue_windows: [ruleOf(NEW_WINDOW)] };
    expect(
      statusLine(
        both,
        state(true, "2026-11-18T08:00", { limit_until: "2026-11-18T08:00", limit_then: 5 * MB }),
        NOW,
      ),
    ).toBe("Queue open. The window closes at Wed 08:00, then a 5 MB/s cap applies.");
    both.speed_entries[0].enabled = false;
    expect(statusLine(both, state(true, "2026-11-18T08:00"), NOW)).toBe(
      "Queue open. The window closes at Wed 08:00.",
    );
  });
});
