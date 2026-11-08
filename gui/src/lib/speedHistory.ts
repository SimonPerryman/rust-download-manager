// The last minute of speed, sampled from the snapshots once a second: one ring
// per download plus the total, for the Speed tab's graph and the sidebar
// sparkline. Presentation only: the engine's `bytes_per_sec` is the source.

import type { DownloadSnapshot } from "./types";

/** Samples kept: one a second for the last 60 s. */
export const HISTORY_LEN = 60;
export const SAMPLE_MS = 1_000;

export interface SpeedHistory {
  /** Total speed, oldest first, at most HISTORY_LEN samples. */
  total: readonly number[];
  byId: ReadonlyMap<number, readonly number[]>;
}

export const EMPTY_HISTORY: SpeedHistory = { total: [], byId: new Map() };

const push = (ring: readonly number[], v: number) => {
  const next = ring.length >= HISTORY_LEN ? ring.slice(ring.length - HISTORY_LEN + 1) : [...ring];
  next.push(v);
  return next;
};

const idle = (ring: readonly number[]) => ring.every((v) => v === 0);

const speedOf = (d: DownloadSnapshot) => (d.status === "downloading" ? d.bytes_per_sec : 0);

/**
 * Add one sample for every download in `ds` (removed ones drop out). Returns `h`
 * itself when nothing is downloading and every ring is already flat, so an idle
 * app doesn't re-render once a second.
 */
export function pushSample(h: SpeedHistory, ds: readonly DownloadSnapshot[]): SpeedHistory {
  const quiet = ds.every((d) => speedOf(d) === 0);
  const sameIds = ds.length === h.byId.size && ds.every((d) => h.byId.has(d.id));
  if (quiet && sameIds && idle(h.total) && [...h.byId.values()].every(idle)) return h;
  const byId = new Map<number, readonly number[]>();
  let total = 0;
  for (const d of ds) {
    const v = speedOf(d);
    total += v;
    byId.set(d.id, push(h.byId.get(d.id) ?? [], v));
  }
  return { total: push(h.total, total), byId };
}

/** Average and peak over the samples that moved any bytes; null when none did. */
export function speedStats(samples: readonly number[]): { average: number; peak: number } | null {
  const moving = samples.filter((v) => v > 0);
  if (moving.length === 0) return null;
  return {
    average: moving.reduce((a, b) => a + b, 0) / moving.length,
    peak: Math.max(...moving),
  };
}

/** The smallest 1, 2 or 5 times a power of ten at or above `v` (the graph's top line). */
export function niceMax(v: number): number {
  if (!(v > 0)) return 1;
  const p = 10 ** Math.floor(Math.log10(v));
  const step = [1, 2, 5, 10].find((k) => k * p >= v) ?? 10;
  return step * p;
}

export interface Plot {
  width: number;
  height: number;
  /** Inner margins: the line is drawn inside them. */
  top: number;
  bottom: number;
  left: number;
  right: number;
  max: number;
}

/** The y of a value inside the plot. */
export function plotY(p: Plot, v: number): number {
  const h = p.height - p.top - p.bottom;
  return p.top + h * (1 - Math.min(v, p.max) / p.max);
}

/**
 * SVG path for the samples, newest at the right edge, so a short history fills
 * in from the right; "" with fewer than two samples.
 */
export function linePath(samples: readonly number[], p: Plot): string {
  if (samples.length < 2) return "";
  const w = p.width - p.left - p.right;
  const off = HISTORY_LEN - samples.length;
  const x = (i: number) => p.left + (w * (i + off)) / (HISTORY_LEN - 1);
  return samples
    .map((v, i) => `${i ? "L" : "M"}${x(i).toFixed(1)} ${plotY(p, v).toFixed(1)}`)
    .join("");
}

/** `linePath` closed down to the baseline, for the soft fill under the line. */
export function areaPath(samples: readonly number[], p: Plot): string {
  const line = linePath(samples, p);
  if (!line) return "";
  const w = p.width - p.left - p.right;
  const off = HISTORY_LEN - samples.length;
  const first = p.left + (w * off) / (HISTORY_LEN - 1);
  const base = p.height - p.bottom;
  return `${line}L${(p.left + w).toFixed(1)} ${base}L${first.toFixed(1)} ${base}Z`;
}
