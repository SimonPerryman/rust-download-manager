import { beforeEach, describe, expect, it } from "vitest";
import { setHour12 } from "./clock";
import {
  dayGroups,
  eventLine,
  eventTime,
  kindLabel,
  mergeEvents,
  readLogWithEvents,
} from "./history";
import type { HistoryEvent, LogLine } from "./types";

const line = (at: number, message = "x"): LogLine => ({ at, level: "info", message, fields: "" });
const event = (at: number, over: Partial<HistoryEvent> = {}): HistoryEvent => ({
  at,
  kind: "held",
  details: "d",
  ...over,
});

beforeEach(() => setHour12(false));

describe("kindLabel", () => {
  it("names the type column, with both tag kinds as Tag", () => {
    expect(kindLabel("held")).toBe("Held");
    expect(kindLabel("tag_added")).toBe("Tag");
    expect(kindLabel("tag_removed")).toBe("Tag");
  });
});

describe("mergeEvents", () => {
  it("puts events among the lines by time, a line first at the same instant", () => {
    const lines = [line(10, "a"), line(30, "b")];
    const events = [event(5), event(30, { details: "tie" }), event(40)];
    const merged = mergeEvents(lines, events);
    expect(merged.map((l) => l.at)).toEqual([5, 10, 30, 30, 40]);
    expect(merged[2].message).toBe("b");
    expect(merged[3]).toEqual(eventLine(events[1]));
  });

  it("marks events with their kind and leaves engine lines unmarked", () => {
    const merged = mergeEvents([line(1)], [event(2, { kind: "tag_added" })]);
    expect(merged.map((l) => l.kind)).toEqual([undefined, "tag_added"]);
  });

  it("returns the lines as they are with no events", () => {
    const lines = [line(1)];
    expect(mergeEvents(lines, [])).toEqual(lines);
  });
});

describe("readLogWithEvents", () => {
  it("merges both reads", async () => {
    const merged = await readLogWithEvents(
      7,
      async (id) => [line(id)],
      async () => [event(1)],
    );
    expect(merged.map((l) => l.at)).toEqual([1, 7]);
  });

  it("keeps the lines when the events can't be read", async () => {
    const merged = await readLogWithEvents(
      7,
      async () => [line(1)],
      () => Promise.reject(new Error("no")),
    );
    expect(merged).toHaveLength(1);
  });
});

describe("eventTime", () => {
  it("shows the local time of day, zero-padded", () => {
    expect(eventTime(new Date(2026, 10, 8, 9, 4).getTime())).toBe("09:04");
  });

  it("follows a 12-hour clock", () => {
    setHour12(true);
    expect(eventTime(new Date(2026, 10, 8, 22, 0).getTime())).toBe("10:00 PM");
  });
});

describe("dayGroups", () => {
  const now = new Date(2026, 10, 20, 12, 0);
  const at = (day: number, h: number, m = 0) => new Date(2026, 10, day, h, m).getTime();

  it("groups by local day, newest first, with Today and Yesterday named", () => {
    const groups = dayGroups(
      [event(at(18, 22)), event(at(20, 8)), event(at(19, 22)), event(at(20, 6))],
      now,
    );
    expect(groups.map((g) => g.events.length)).toEqual([2, 1, 1]);
    expect(groups.slice(0, 2).map((g) => g.heading)).toEqual(["Today", "Yesterday"]);
    expect(groups[0].events.map((e) => e.at)).toEqual([at(20, 8), at(20, 6)]);
  });

  it("is empty for no events", () => {
    expect(dayGroups([], now)).toEqual([]);
  });
});
