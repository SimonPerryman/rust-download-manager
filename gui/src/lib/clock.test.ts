import { describe, expect, it } from "vitest";
import { normalizeTime, setHour12, timeLabel } from "./clock";

describe("normalizeTime", () => {
  it("reads the ways people type a 24-hour time", () => {
    for (const [typed, want] of [
      ["22:00", "22:00"],
      ["930", "09:30"],
      ["0930", "09:30"],
      ["9:30", "09:30"],
      ["9.30", "09:30"],
      ["09:30", "09:30"],
      ["9", "09:00"],
      ["22", "22:00"],
      ["2200", "22:00"],
      ["0", "00:00"],
      [" 7:05 ", "07:05"],
    ]) {
      expect(normalizeTime(typed), typed).toBe(want);
    }
  });

  it("reads am and pm", () => {
    for (const [typed, want] of [
      ["930pm", "21:30"],
      ["9:30 pm", "21:30"],
      ["9:30 PM", "21:30"],
      ["9:30am", "09:30"],
      ["9 am", "09:00"],
      ["12am", "00:00"],
      ["12:15 am", "00:15"],
      ["12pm", "12:00"],
      ["1230pm", "12:30"],
      ["9 p.m.", "21:00"],
    ]) {
      expect(normalizeTime(typed), typed).toBe(want);
    }
  });

  it("refuses what isn't a time", () => {
    for (const typed of [
      "",
      "abc",
      "24:00",
      "2460",
      "9:5",
      "9:60",
      "930930",
      "13pm",
      "0pm",
      "9 m",
      "9:30:00",
      "-9",
      "9.5",
      "pm",
    ]) {
      expect(normalizeTime(typed), typed).toBeNull();
    }
  });
});

describe("timeLabel", () => {
  it("leaves 24-hour times as they are", () => {
    expect(timeLabel("22:00", false)).toBe("22:00");
    expect(timeLabel("00:05", false)).toBe("00:05");
  });

  it("words 12-hour times with AM and PM", () => {
    expect(timeLabel("22:00", true)).toBe("10:00 PM");
    expect(timeLabel("00:05", true)).toBe("12:05 AM");
    expect(timeLabel("12:00", true)).toBe("12:00 PM");
    expect(timeLabel("09:30", true)).toBe("9:30 AM");
  });

  it("follows the pinned preference when none is passed", () => {
    setHour12(true);
    expect(timeLabel("22:00")).toBe("10:00 PM");
    setHour12(false);
    expect(timeLabel("22:00")).toBe("22:00");
  });
});
