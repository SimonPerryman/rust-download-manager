import { describe, expect, it } from "vitest";
import {
  areaPath,
  EMPTY_HISTORY,
  HISTORY_LEN,
  linePath,
  niceMax,
  plotY,
  pushSample,
  speedStats,
  type Plot,
} from "./speedHistory";
import { snap } from "./testSnapshot";

const running = (id: number, bps: number) =>
  snap({ id, status: "downloading", bytes_per_sec: bps });

describe("pushSample", () => {
  it("samples each download and the total", () => {
    const h = pushSample(EMPTY_HISTORY, [running(1, 100), running(2, 50), snap({ id: 3 })]);
    expect(h.total).toEqual([150]);
    expect(h.byId.get(1)).toEqual([100]);
    expect(h.byId.get(3)).toEqual([0]);
  });

  it("counts only downloading rows", () => {
    const paused = snap({ id: 1, status: "paused", bytes_per_sec: 999 });
    expect(pushSample(EMPTY_HISTORY, [paused, running(2, 1)]).total).toEqual([1]);
  });

  it("keeps the last minute only", () => {
    let h = EMPTY_HISTORY;
    for (let i = 1; i <= HISTORY_LEN + 5; i++) h = pushSample(h, [running(1, i)]);
    expect(h.total).toHaveLength(HISTORY_LEN);
    expect(h.total[0]).toBe(6);
    expect(h.total[h.total.length - 1]).toBe(HISTORY_LEN + 5);
  });

  it("drops removed downloads", () => {
    const h = pushSample(pushSample(EMPTY_HISTORY, [running(1, 5), running(2, 5)]), [
      running(2, 5),
    ]);
    expect([...h.byId.keys()]).toEqual([2]);
  });

  it("keeps sampling zeros after a download stops, so the graph runs down", () => {
    let h = pushSample(EMPTY_HISTORY, [running(1, 10)]);
    h = pushSample(h, [snap({ id: 1, status: "paused" })]);
    expect(h.byId.get(1)).toEqual([10, 0]);
  });

  it("returns the same history when fully idle", () => {
    const h = pushSample(EMPTY_HISTORY, [snap({ id: 1 })]);
    expect(pushSample(h, [snap({ id: 1 })])).toBe(h);
  });

  it("still notices a new download while idle", () => {
    const h = pushSample(EMPTY_HISTORY, [snap({ id: 1 })]);
    expect(pushSample(h, [snap({ id: 1 }), snap({ id: 2 })]).byId.has(2)).toBe(true);
  });
});

describe("speedStats", () => {
  it("averages and peaks over moving samples", () => {
    expect(speedStats([0, 10, 30, 0])).toEqual({ average: 20, peak: 30 });
  });

  it("is null when nothing moved", () => {
    expect(speedStats([0, 0])).toBeNull();
    expect(speedStats([])).toBeNull();
  });
});

describe("niceMax", () => {
  it("rounds up to 1, 2 or 5 times a power of ten", () => {
    expect(niceMax(7)).toBe(10);
    expect(niceMax(130)).toBe(200);
    expect(niceMax(4_200_000)).toBe(5_000_000);
    expect(niceMax(1000)).toBe(1000);
    expect(niceMax(0)).toBe(1);
  });
});

describe("plot geometry", () => {
  const p: Plot = { width: 120, height: 100, top: 0, bottom: 0, left: 0, right: 0, max: 10 };

  it("maps values inside the plot, clamping above the top", () => {
    expect(plotY(p, 0)).toBe(100);
    expect(plotY(p, 10)).toBe(0);
    expect(plotY(p, 50)).toBe(0);
  });

  it("puts the newest sample at the right edge", () => {
    const d = linePath([0, 10], p);
    expect(d.endsWith("L120.0 0.0")).toBe(true);
    expect(d.startsWith("M")).toBe(true);
  });

  it("draws nothing for fewer than two samples", () => {
    expect(linePath([5], p)).toBe("");
    expect(areaPath([], p)).toBe("");
  });

  it("closes the area down to the baseline", () => {
    expect(areaPath([0, 10], p).endsWith("Z")).toBe(true);
  });
});
