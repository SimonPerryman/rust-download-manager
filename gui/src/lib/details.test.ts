import { describe, expect, it } from "vitest";
import {
  checksumView,
  datesLine,
  formatWhen,
  isSha256,
  verifyErrorText,
  clampPaneHeight,
  effectiveLimit,
  formatElapsed,
  PANE_MIN_H,
  statePhrase,
  summaryParts,
} from "./details";
import { snap } from "./testSnapshot";

const MB = 1024 * 1024;

describe("formatElapsed", () => {
  it("reads like speech at every scale", () => {
    expect(formatElapsed(400)).toBe("under 1 s");
    expect(formatElapsed(48_900)).toBe("48 s");
    expect(formatElapsed(134_000)).toBe("2 min 14 s");
    expect(formatElapsed(120_000)).toBe("2 min");
    expect(formatElapsed(3_900_000)).toBe("1 h 5 min");
    expect(formatElapsed(7_200_000)).toBe("2 h");
  });
});

describe("summaryParts", () => {
  it("gives size, time, average and the checksum", () => {
    const d = snap({
      status: "completed",
      total: 100 * MB,
      downloaded: 100 * MB,
      active_ms: 10_000,
      checksum_state: "verified",
    });
    expect(summaryParts(d)).toEqual({
      size: "100.0 MB",
      rest: "in 10 s, average 10.0 MB/s, checksum verified",
    });
  });

  it("averages a sub-second run over its real time", () => {
    const d = snap({ status: "completed", total: 2048, downloaded: 2048, active_ms: 50 });
    expect(summaryParts(d).rest).toBe("in under 1 s, average 40.0 KB/s, no checksum given");
  });

  it("is a plain line when the engine doesn't know the active time", () => {
    const d = snap({ status: "completed", total: 2048, downloaded: 2048, active_ms: null });
    expect(summaryParts(d)).toEqual({ size: "2.00 KB", rest: "no checksum given" });
  });

  it("falls back to the bytes when the size was never known", () => {
    const d = snap({
      status: "completed",
      total: null,
      downloaded: 512,
      checksum_state: "mismatch",
    });
    expect(summaryParts(d)).toEqual({ size: "512 B", rest: "checksum doesn't match" });
  });
});

describe("statePhrase", () => {
  it("covers every status", () => {
    expect(statePhrase(snap({ status: "downloading", downloaded: 421, total: 1000 }))).toBe(
      "42.1% of 1000 B",
    );
    expect(statePhrase(snap({ status: "downloading", downloaded: 2048 }))).toBe("2.00 KB so far");
    expect(statePhrase(snap({ status: "queued" }))).toBe("Waiting for a free slot");
    expect(statePhrase(snap({ status: "paused" }))).toBe("Paused");
    expect(statePhrase(snap({ status: "completed" }))).toBe("Completed");
    expect(statePhrase(snap({ status: "failed" }))).toBe("Failed");
    expect(statePhrase(snap({ status: "cancelled", partial_kept: true }))).toBe(
      "Stopped, partial file kept",
    );
    expect(statePhrase(snap({ status: "cancelled" }))).toBe("Stopped");
  });
});

describe("checksumView", () => {
  const hash = "ab".repeat(32);

  it("offers Verify on a finished file with no checksum", () => {
    const v = checksumView(snap({ status: "completed" }));
    expect(v).toMatchObject({ text: "No checksum given", canVerify: true, hash: null });
  });

  it("explains that Verify comes once it's done", () => {
    const v = checksumView(snap({ status: "downloading" }));
    expect(v).toMatchObject({ canVerify: false, hint: "You can verify one once it's done." });
  });

  it("shows the expected hash without its prefix", () => {
    const v = checksumView(snap({ status: "downloading", checksum: `sha256:${hash}` }));
    expect(v).toMatchObject({ text: "Checks SHA-256 when done", hash });
  });

  it("follows the engine's state", () => {
    const d = { status: "completed" as const, checksum: `sha256:${hash}` };
    expect(checksumView(snap({ ...d, checksum_state: "pending" }))).toMatchObject({
      tone: "checking",
      canVerify: false,
    });
    expect(checksumView(snap({ ...d, checksum_state: "verified" }))).toMatchObject({
      tone: "ok",
      text: "SHA-256 verified",
      canVerify: false,
    });
    expect(checksumView(snap({ ...d, checksum_state: "mismatch" }))).toMatchObject({
      tone: "bad",
      canVerify: true,
    });
    expect(checksumView(snap({ ...d, checksum_state: "none" }))).toMatchObject({
      text: "Not checked yet",
      canVerify: true,
    });
  });
});

describe("effectiveLimit", () => {
  it("is the lower of the two, or whichever is set", () => {
    expect(effectiveLimit(null, null)).toBeNull();
    expect(effectiveLimit(5, null)).toBe(5);
    expect(effectiveLimit(null, 7)).toBe(7);
    expect(effectiveLimit(9, 7)).toBe(7);
  });
});

describe("clampPaneHeight", () => {
  it("keeps the pane between its minimum and what leaves the table usable", () => {
    expect(clampPaneHeight(10, 760)).toBe(PANE_MIN_H);
    expect(clampPaneHeight(300, 760)).toBe(300);
    expect(clampPaneHeight(900, 760)).toBe(560);
    expect(clampPaneHeight(300, 300)).toBe(PANE_MIN_H);
  });
});

describe("isSha256", () => {
  it("takes 64 hex digits, with or without the prefix", () => {
    const hex = "0123456789abcdef".repeat(4);
    expect(isSha256(hex)).toBe(true);
    expect(isSha256(` sha256:${hex.toUpperCase()} `)).toBe(true);
    expect(isSha256(hex.slice(1))).toBe(false);
    expect(isSha256("md5:abc")).toBe(false);
    expect(isSha256("")).toBe(false);
  });
});

describe("verifyErrorText", () => {
  it("tidies the engine's reason into a sentence", () => {
    expect(verifyErrorText("invalid request: download 3's file is no longer on disk")).toBe(
      "Couldn't verify the file. Download 3's file is no longer on disk.",
    );
    expect(verifyErrorText(new Error("busy."))).toBe("Couldn't verify the file. Busy.");
    expect(verifyErrorText("")).toBe("Couldn't verify the file.");
  });
});

describe("formatWhen / datesLine", () => {
  const now = new Date(2026, 10, 7, 15, 0);
  const at = (day: number, h: number, m: number) => new Date(2026, 10, day, h, m).getTime() / 1000;

  it("says today, yesterday or the date, with the time", () => {
    expect(formatWhen(at(7, 14, 5), now)).toBe("today at 14:05");
    expect(formatWhen(at(6, 9, 12), now)).toBe("yesterday at 09:12");
    expect(formatWhen(at(3, 8, 0), now)).toBe("3 Nov at 08:00");
  });

  it("adds when it ended, for finished, failed and stopped downloads", () => {
    const added = at(7, 14, 5);
    const finished = at(7, 14, 7);
    expect(
      datesLine(snap({ status: "completed", added_at: added, finished_at: finished }), now),
    ).toBe("Added today at 14:05, finished today at 14:07");
    expect(
      datesLine(snap({ status: "cancelled", added_at: added, finished_at: finished }), now),
    ).toBe("Added today at 14:05, stopped today at 14:07");
    expect(datesLine(snap({ status: "downloading", added_at: added }), now)).toBe(
      "Added today at 14:05",
    );
    expect(datesLine(snap({ status: "failed", finished_at: finished }), now)).toBe(
      "Failed today at 14:07",
    );
    expect(datesLine(snap({ status: "queued" }), now)).toBe("");
  });
});
