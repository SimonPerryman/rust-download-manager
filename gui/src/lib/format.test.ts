import { describe, expect, it } from "vitest";
import { formatAdded, formatLeft, statusCountsLine } from "./format";
describe("statusCountsLine", () => {
  it("names what's downloading and waiting, or says nothing is", () => {
    expect(statusCountsLine(2, 1)).toBe("2 downloading, 1 waiting");
    expect(statusCountsLine(1, 0)).toBe("1 downloading");
    expect(statusCountsLine(0, 1)).toBe("1 waiting");
    expect(statusCountsLine(0, 0)).toBe("Nothing downloading");
  });
});

describe("formatAdded", () => {
  const now = new Date(2026, 10, 7, 18, 30);
  const secs = (d: Date) => d.getTime() / 1000;

  it("shows the time for today, Yesterday, else day and month", () => {
    expect(formatAdded(secs(new Date(2026, 10, 7, 14, 5)), now)).toBe("14:05");
    expect(formatAdded(secs(new Date(2026, 10, 7, 0, 0)), now)).toBe("00:00");
    expect(formatAdded(secs(new Date(2026, 10, 6, 23, 59)), now)).toBe("Yesterday");
    expect(formatAdded(secs(new Date(2026, 10, 6, 0, 0)), now)).toBe("Yesterday");
    expect(formatAdded(secs(new Date(2026, 10, 3, 9, 0)), now)).toBe("3 Nov");
    expect(formatAdded(secs(new Date(2025, 0, 12, 9, 0)), now)).toBe("12 Jan");
  });

  it("handles Yesterday across a month boundary", () => {
    const first = new Date(2026, 11, 1, 8, 0);
    expect(formatAdded(secs(new Date(2026, 10, 30, 22, 0)), first)).toBe("Yesterday");
  });

  it("is blank when unknown", () => {
    expect(formatAdded(null, now)).toBe("");
  });
});

describe("formatLeft", () => {
  it("uses seconds, minutes, then hours and minutes", () => {
    expect(formatLeft(9)).toBe("9 s");
    expect(formatLeft(59)).toBe("59 s");
    expect(formatLeft(60)).toBe("1 min");
    expect(formatLeft(12 * 60 + 40)).toBe("12 min");
    expect(formatLeft(3600)).toBe("1 h");
    expect(formatLeft(3600 + 5 * 60 + 30)).toBe("1 h 5 min");
  });

  it("is blank when unknown", () => {
    expect(formatLeft(null)).toBe("");
  });
});
