import { describe, expect, it } from "vitest";
import {
  canSchedule,
  missedText,
  queuedUntilLabel,
  rescheduleChoices,
  scheduledForLabel,
  startFromText,
  startGroups,
  startsLabel,
  startState,
  wasDueLabel,
  windowState,
} from "./startAt";
import { setHour12 } from "./clock";
import { snap } from "./testSnapshot";

// Monday 9 March 2026, 21:00 local.
const MON_21 = new Date(2026, 2, 9, 21, 0);

describe("a typed start time", () => {
  it("is the next such time", () => {
    expect(startFromText("22:00", MON_21)).toBe("2026-03-09T22:00");
    expect(startFromText("21:00", MON_21)).toBe("2026-03-09T21:00");
    expect(startFromText(" 01:30 ", MON_21)).toBe("2026-03-10T01:30");
  });

  it("is refused when it isn't a time", () => {
    for (const bad of ["", "25:00", "9:60", "noon", "13pm"]) {
      expect(startFromText(bad, MON_21), bad).toBeNull();
    }
  });

  it("accepts the short ways of typing one", () => {
    expect(startFromText("930pm", MON_21)).toBe("2026-03-09T21:30");
    expect(startFromText("9:30 pm", MON_21)).toBe("2026-03-09T21:30");
    expect(startFromText("9", MON_21)).toBe("2026-03-10T09:00");
    expect(startFromText("1.15", MON_21)).toBe("2026-03-10T01:15");
  });
});

describe("start-at wording", () => {
  it("says today with the clock, else the weekday and clock, never tomorrow", () => {
    expect(startsLabel("2026-03-09T22:00", MON_21)).toBe("Starts today 22:00");
    expect(startsLabel("2026-03-10T22:00", MON_21)).toBe("Starts Tue 22:00");
    expect(startsLabel("2026-03-12T03:00", MON_21)).toBe("Starts Thu 03:00");
    expect(startsLabel("2026-03-24T22:00", MON_21)).toBe("Starts 24 Mar 22:00");
  });

  it("says when a missed one was due", () => {
    const tue = new Date(2026, 2, 10, 9, 0);
    expect(wasDueLabel("2026-03-09T22:00", tue)).toBe("Was due Mon 22:00");
    expect(wasDueLabel("2026-03-10T03:00", tue)).toBe("Was due today 03:00");
  });

  it("labels a waiting row by its time of day, and its weekday when not today", () => {
    expect(scheduledForLabel("2026-03-09T22:00", MON_21)).toBe("Scheduled for 22:00");
    expect(scheduledForLabel("2026-03-12T01:00", MON_21)).toBe("Scheduled for Thu 01:00");
  });

  it("shows a row's state only while it is held", () => {
    const held = snap({ start_at: "2026-03-09T22:00", start_group: 1 });
    expect(startState(held, MON_21)).toBe("Scheduled for 22:00");
    expect(startState({ ...held, start_missed: true }, MON_21)).toBe("Was due today 22:00");
    expect(startState(snap(), MON_21)).toBeNull();
  });

  it("says when a closed queue window holds a waiting row, by time of day and weekday", () => {
    expect(queuedUntilLabel("2026-03-09T22:00", MON_21)).toBe("Queued until 22:00");
    expect(queuedUntilLabel("2026-03-10T08:00", MON_21)).toBe("Queued until Tue 08:00");
    const held = snap({ window_until: "2026-03-09T22:00" });
    expect(windowState(held, MON_21)).toBe("Queued until 22:00");
    expect(windowState(snap(), MON_21)).toBeNull();
    expect(windowState({ ...held, status: "paused" }, MON_21)).toBeNull();
    expect(windowState({ ...held, start_at: "2026-03-09T23:00" }, MON_21)).toBeNull();
  });

  it("words times on a 12-hour clock when the OS uses one", () => {
    setHour12(true);
    try {
      expect(scheduledForLabel("2026-03-09T22:00", MON_21)).toBe("Scheduled for 10:00 PM");
      expect(queuedUntilLabel("2026-03-10T01:00", MON_21)).toBe("Queued until Tue 1:00 AM");
      expect(rescheduleChoices(MON_21)[0].when).toBe("Mon 10:00 PM");
    } finally {
      setHour12(false);
    }
  });

  it("offers a time to anything not running or finished", () => {
    for (const s of ["queued", "paused", "failed"] as const) {
      expect(canSchedule(snap({ status: s })), s).toBe(true);
    }
    expect(canSchedule(snap({ status: "cancelled", partial_kept: true }))).toBe(true);
    for (const s of ["downloading", "completed", "cancelled"] as const) {
      expect(canSchedule(snap({ status: s })), s).toBe(false);
    }
    expect(canSchedule(snap({ status: "failed", archived: true }))).toBe(false);
  });
});

describe("start groups", () => {
  const held = (id: number, group: number, at: string, extra = {}) =>
    snap({ id, start_at: at, start_group: group, ...extra });

  it("gathers the downloads that share a schedule, soonest first", () => {
    const ds = [
      held(1, 7, "2026-03-09T22:00", { batch: 1, batch_label: "Season 2" }),
      held(2, 7, "2026-03-09T22:00", { batch: 1, batch_label: "Season 2" }),
      held(3, 8, "2026-03-09T20:00", { filename: "backup.zip" }),
      snap({ id: 4 }),
      held(5, 9, "2026-03-09T23:00", { status: "paused" }),
    ];
    expect(startGroups(ds)).toEqual([
      {
        group: 8,
        at: "2026-03-09T20:00",
        missed: false,
        ids: [3],
        name: "backup.zip",
        count: null,
      },
      {
        group: 7,
        at: "2026-03-09T22:00",
        missed: false,
        ids: [1, 2],
        name: "Season 2",
        count: "2 files",
      },
    ]);
  });

  it("leads the missed notice with schedules and puts downloads in brackets", () => {
    const g = (ids: number[]) => ({
      group: 1,
      at: "x",
      missed: true,
      ids,
      name: "n",
      count: null,
    });
    expect(missedText([g([1]), g(Array.from({ length: 49 }, (_, i) => i)), g([2])])).toBe(
      "3 scheduled starts were missed (51 downloads)",
    );
    expect(missedText([g([1])])).toBe("1 scheduled start was missed (1 download)");
  });
});

describe("the Reschedule menu", () => {
  it("lands on real times: tonight, tomorrow, next week", () => {
    const morning = new Date(2026, 10, 24, 9, 0); // Tue 24 Nov
    expect(rescheduleChoices(morning)).toEqual([
      { when: "Tue 22:00", name: "Tonight", at: "2026-11-24T22:00" },
      { when: "Wed 22:00", name: "Tomorrow", at: "2026-11-25T22:00" },
      { when: "1 Dec 22:00", name: "Next week", at: "2026-12-01T22:00" },
    ]);
  });

  it("leaves out tonight once it has passed", () => {
    const late = new Date(2026, 10, 24, 22, 30);
    expect(rescheduleChoices(late).map((c) => c.name)).toEqual(["Tomorrow", "Next week"]);
  });
});
