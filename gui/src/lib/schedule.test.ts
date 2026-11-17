import { describe, expect, it } from "vitest";
import {
  autoName,
  draftProblem,
  NEW_WINDOW,
  pausedNote,
  ruleName,
  ruleOf,
  statusLine,
  summary,
  validTime,
  whenLabel,
  withCopy,
} from "./schedule";
import type { Schedule } from "./types";

// 2026-11-17 is a Tuesday.
const NOW = new Date(2026, 10, 17, 12, 0);

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
    expect(pausedNote({ open: false, until: "2026-11-17T22:00" }, NOW)).toBe(
      "Queue paused until 22:00",
    );
    expect(pausedNote({ open: true, until: "2026-11-17T22:00" }, NOW)).toBeNull();
    expect(pausedNote(null, NOW)).toBeNull();
  });

  it("states the queue in one line", () => {
    const none: Schedule = { queue_windows: [], speed_entries: [] };
    expect(statusLine(none, { open: true, until: null }, NOW)).toBe(
      "No schedules. Downloads run any time.",
    );
    const on = { ...none, queue_windows: [ruleOf(NEW_WINDOW)] };
    expect(statusLine(on, { open: false, until: "2026-11-17T22:00" }, NOW)).toBe(
      "Queue paused. The window opens at 22:00.",
    );
    expect(statusLine(on, { open: true, until: "2026-11-18T08:00" }, NOW)).toBe(
      "Queue open. The window closes at Wed 08:00.",
    );
    on.queue_windows[0].enabled = false;
    expect(statusLine(on, { open: true, until: null }, NOW)).toMatch(/^All schedules are off/);
  });
});

describe("copy", () => {
  it("puts a copy right after the rule", () => {
    const s: Schedule = {
      queue_windows: [ruleOf(NEW_WINDOW), ruleOf({ ...NEW_WINDOW, start: "01:00" })],
      speed_entries: [],
    };
    const next = withCopy(s, 0);
    expect(next.queue_windows).toHaveLength(3);
    expect(next.queue_windows[1]).toEqual(next.queue_windows[0]);
    expect(next.queue_windows[1]).not.toBe(next.queue_windows[0]);
  });
});
