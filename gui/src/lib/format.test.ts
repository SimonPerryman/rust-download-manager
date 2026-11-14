import { describe, expect, it } from "vitest";
import { formatAdded, formatBytes, formatLeft, statusCountsLine } from "./format";
const MiB = 1024 * 1024;

describe("formatBytes", () => {
  it("keeps two decimals under 10 and one from 10 up", () => {
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1024)).toBe("1.00 KB");
    expect(formatBytes(9.99 * MiB)).toBe("9.99 MB");
    expect(formatBytes(99.9 * MiB)).toBe("99.9 MB");
    expect(formatBytes(100 * MiB)).toBe("100.0 MB");
    expect(formatBytes(370 * MiB)).toBe("370.0 MB");
    expect(formatBytes(999.9 * MiB)).toBe("999.9 MB");
  });

  it("moves up a unit at 1024, through TB", () => {
    expect(formatBytes(1023.9 * MiB)).toBe("1023.9 MB");
    expect(formatBytes(1024 * MiB)).toBe("1.00 GB");
    expect(formatBytes(999.9 * 1024 * MiB)).toBe("999.9 GB");
    expect(formatBytes(1.2 * 1024 * 1024 * MiB)).toBe("1.20 TB");
    expect(formatBytes(2048 * 1024 * 1024 * MiB)).toBe("2048.0 TB");
  });

  // The Size column (.c-size in styles/table.css) is sized for "1023.9 MB"; a longer string would clip.
  it("never runs past nine characters below a petabyte", () => {
    const edges = [1023, 1023.9 * 1024, 1023.9 * MiB, 1023.9 * 1024 * MiB, 1023.9 * 1024 ** 4];
    for (const bytes of edges) expect(formatBytes(bytes).length).toBeLessThanOrEqual(9);
  });
});

describe("statusCountsLine", () => {
  it("names what's downloading and waiting, or says nothing is", () => {
    expect(statusCountsLine(2, 1)).toBe("2 downloading, 1 queued");
    expect(statusCountsLine(1, 0)).toBe("1 downloading");
    expect(statusCountsLine(0, 1)).toBe("1 queued");
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
