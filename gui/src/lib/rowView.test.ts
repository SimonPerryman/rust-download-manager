import { describe, expect, it } from "vitest";
import {
  isPausable,
  isRemovable,
  isResumable,
  isStoppable,
  limitView,
  continues,
  toResume,
  pauseAllMode,
  precisePercent,
  progressView,
  rowActions,
  sizeText,
  totalSpeed,
} from "./rowView";
import { snap } from "./testSnapshot";
import type { DownloadSnapshot } from "./types";

const labels = (d: DownloadSnapshot, lingering = false) =>
  rowActions(d, lingering).map((a) => `${a.label}:${a.style}`);

describe("precisePercent", () => {
  it("is one decimal, rounded down, capped at 100", () => {
    expect(precisePercent(snap({ downloaded: 421, total: 1000 }))).toBe(42.1);
    expect(precisePercent(snap({ downloaded: 9999, total: 10000 }))).toBe(99.9);
    expect(precisePercent(snap({ downloaded: 12, total: 10 }))).toBe(100);
  });

  it("is null without a known total", () => {
    expect(precisePercent(snap({ downloaded: 5, total: null }))).toBeNull();
    expect(precisePercent(snap({ downloaded: 5, total: 0 }))).toBeNull();
  });
});

describe("progressView", () => {
  it("shows a live bar with one decimal while downloading", () => {
    expect(progressView(snap({ status: "downloading", downloaded: 421, total: 1000 }))).toEqual({
      kind: "bar",
      pct: 42.1,
      text: "42.1%",
      muted: false,
    });
  });

  it("shows the bytes so far while downloading without a length", () => {
    expect(progressView(snap({ status: "downloading", downloaded: 2048 }))).toEqual({
      kind: "text",
      text: "2.00 KB",
      tone: "plain",
    });
  });

  it("mutes the bar of paused and kept-stopped rows, whole percent", () => {
    const paused = progressView(snap({ status: "paused", downloaded: 446, total: 1000 }));
    expect(paused).toEqual({ kind: "bar", pct: 44.6, text: "44%", muted: true });
    const kept = snap({ status: "cancelled", partial_kept: true, downloaded: 27, total: 100 });
    expect(progressView(kept)).toEqual({ kind: "bar", pct: 27, text: "27%", muted: true });
  });

  it("uses a word for rows that aren't moving", () => {
    expect(progressView(snap({ status: "queued" }))).toMatchObject({ text: "Waiting" });
    expect(progressView(snap({ status: "paused" }))).toMatchObject({ text: "Paused" });
    expect(progressView(snap({ status: "completed" }))).toMatchObject({
      text: "Completed",
      tone: "done",
    });
    expect(progressView(snap({ status: "failed" }))).toMatchObject({
      text: "Failed",
      tone: "failed",
    });
    const deleted = snap({ status: "cancelled", downloaded: 27, total: 100 });
    expect(progressView(deleted)).toMatchObject({ text: "Stopped", tone: "stopped" });
  });
});

describe("sizeText", () => {
  it("is the total, blank while unknown", () => {
    expect(sizeText(snap({ total: 1024 * 1024 }))).toBe("1.00 MB");
    expect(sizeText(snap({ total: null, downloaded: 500 }))).toBe("");
  });
});

describe("rowActions", () => {
  it("offers limit, pause or resume, and stop while a download can still stream", () => {
    const streaming = ["Speed limit:icon", "Pause:icon", "Stop:icon"];
    expect(labels(snap({ status: "downloading" }))).toEqual(streaming);
    expect(labels(snap({ status: "queued" }))).toEqual(streaming);
    expect(labels(snap({ status: "paused" }))).toEqual([
      "Speed limit:icon",
      "Resume:icon",
      "Stop:icon",
    ]);
  });

  it("makes Open the solid primary only while a finished row lingers", () => {
    const done = snap({ status: "completed" });
    expect(labels(done, true)).toEqual(["Open:primary", "Show in folder:icon", "Archive:icon"]);
    expect(labels(done, false)).toEqual(["Open:icon", "Show in folder:icon", "Archive:icon"]);
  });

  it("removes rather than archives an archived row", () => {
    expect(labels(snap({ status: "completed", archived: true }), false)).toEqual([
      "Open:icon",
      "Show in folder:icon",
      "Remove:icon",
    ]);
  });

  it("offers Retry on failed rows", () => {
    expect(labels(snap({ status: "failed" }))).toEqual(["Retry:soft", "Archive:icon"]);
  });

  it("offers Resume and Delete partial file for a kept stop, Download again otherwise", () => {
    expect(labels(snap({ status: "cancelled", partial_kept: true }))).toEqual([
      "Resume:soft",
      "Delete partial file:icon",
      "Archive:icon",
    ]);
    expect(labels(snap({ status: "cancelled" }))).toEqual(["Download again:soft", "Archive:icon"]);
  });
});

describe("pauseAllMode", () => {
  it("pauses while anything runs or waits, else resumes while anything is paused", () => {
    expect(pauseAllMode([snap({ status: "downloading" }), snap({ status: "paused" })])).toBe(
      "pause",
    );
    expect(pauseAllMode([snap({ status: "queued" })])).toBe("pause");
    expect(pauseAllMode([snap({ status: "paused" }), snap({ status: "completed" })])).toBe(
      "resume",
    );
    expect(pauseAllMode([snap({ status: "cancelled" }), snap({ status: "failed" })])).toBeNull();
    expect(pauseAllMode([])).toBeNull();
  });
});

describe("totalSpeed", () => {
  it("sums only downloading rows", () => {
    expect(
      totalSpeed([
        snap({ id: 1, status: "downloading", bytes_per_sec: 100 }),
        snap({ id: 2, status: "downloading", bytes_per_sec: 50 }),
        snap({ id: 3, status: "paused", bytes_per_sec: 999 }),
      ]),
    ).toBe(150);
    expect(totalSpeed([])).toBe(0);
  });
});

describe("isStoppable", () => {
  it("holds for running, waiting and paused rows only", () => {
    expect(isStoppable(snap({ status: "downloading" }))).toBe(true);
    expect(isStoppable(snap({ status: "queued" }))).toBe(true);
    expect(isStoppable(snap({ status: "paused" }))).toBe(true);
    expect(isStoppable(snap({ status: "completed" }))).toBe(false);
    expect(isStoppable(snap({ status: "cancelled" }))).toBe(false);
    expect(isStoppable(undefined)).toBe(false);
  });
});

describe("isPausable / isResumable / isRemovable", () => {
  const all = ["queued", "downloading", "paused", "completed", "failed", "cancelled"] as const;
  const which = (f: (d: DownloadSnapshot) => boolean) =>
    all.filter((status) => f(snap({ status })));

  it("sorts each status into the actions that apply", () => {
    expect(which(isPausable)).toEqual(["queued", "downloading"]);
    expect(which(isResumable)).toEqual(["paused", "cancelled"]);
    expect(which(isRemovable)).toEqual(["completed", "failed", "cancelled"]);
  });
});

describe("toResume", () => {
  it("restarts a lone stopped row but never a stopped row in a bulk Resume", () => {
    const fresh = snap({ id: 1, status: "cancelled" });
    const kept = snap({ id: 2, status: "cancelled", partial_kept: true });
    const paused = snap({ id: 3, status: "paused" });
    expect(toResume([fresh]).map((d) => d.id)).toEqual([1]);
    expect(toResume([fresh, kept, paused]).map((d) => d.id)).toEqual([2, 3]);
    expect(toResume([fresh, snap({ id: 4, status: "completed" })])).toEqual([]);
  });
});

describe("continues", () => {
  it("is true only where a Resume picks up where it left off", () => {
    expect(continues(snap({ status: "paused" }))).toBe(true);
    expect(continues(snap({ status: "cancelled", partial_kept: true }))).toBe(true);
    expect(continues(snap({ status: "cancelled" }))).toBe(false);
    expect(continues(snap({ status: "completed" }))).toBe(false);
  });
});

describe("limitView", () => {
  const LIMIT = 50 * 1024;

  it("reads a streaming row's speed against its limit", () => {
    const v = limitView(snap({ status: "downloading", bytes_per_sec: 49.8 * 1024 }), LIMIT);
    expect(v).toEqual({
      state: "live",
      speed: "49.8",
      limit: "50 KB/s",
      short: "50K",
      label: "49.8 KB/s, limited to 50 KB/s. Change speed limit",
    });
  });

  it("shows a dash against the limit before the first reading", () => {
    const v = limitView(snap({ status: "downloading", bytes_per_sec: 0 }), LIMIT);
    expect(v.state).toBe("waiting");
    expect(v.speed).toBe("–");
    expect(v.label).toBe("No speed yet, limited to 50 KB/s. Change speed limit");
  });

  it("shows the limit alone while paused or waiting", () => {
    for (const status of ["paused", "queued"] as const) {
      const v = limitView(snap({ status, bytes_per_sec: 0 }), LIMIT);
      expect(v.state).toBe("capped");
      expect(v.label).toBe("Limited to 50 KB/s. Change speed limit");
    }
  });
});
