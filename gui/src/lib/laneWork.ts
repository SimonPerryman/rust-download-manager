// The details pane's lane rows: one row per live lane (connection), each
// showing only that lane's own work, so no byte is ever drawn on two rows.
// The engine says only which lane is on each piece right now (`lanes`); which
// lane wrote what is gathered here from successive snapshots. Pure; the
// LaneRows component only renders what this returns.

import { formatBytes } from "./format";
import type { DownloadSnapshot, Segment } from "./types";

/** Lane number for work whose lane has closed, or that this app never saw a lane write. */
export const NO_LANE = 0;

/** How long work folding into the top row is marked as just arrived. */
export const FOLD_MS = 1_500;

/** Bytes `[from, to)` of piece `piece` (an index into `segments`) that lane `lane` wrote. */
export interface Span {
  piece: number;
  from: number;
  to: number;
  lane: number;
  /** When its lane closed and the span moved to the top row (ms since the epoch). */
  foldedAt?: number;
}

/** What one download's lanes have written so far. */
export interface LaneWork {
  spans: readonly Span[];
  /** Where each piece had got at the last snapshot. */
  seen: readonly number[];
  /** The most lanes seen live at once. */
  peak: number;
}

export const EMPTY_WORK: LaneWork = { spans: [], seen: [], peak: 0 };

const pieceLen = (s: Segment) => Math.max(0, s.end - s.start);
const reached = (s: Segment) => s.start + Math.min(s.done, pieceLen(s));
const finished = (s: Segment) => s.done >= pieceLen(s);

/** The lanes live now, lowest first; none unless downloading. */
export function liveLanes(d: DownloadSnapshot): number[] {
  if (d.status !== "downloading") return [];
  const lanes = new Set<number>();
  for (const lane of d.lanes ?? []) if (lane != null) lanes.add(lane);
  return [...lanes].sort((a, b) => a - b);
}

const sameSpan = (a: Span, b: Span) =>
  a.piece === b.piece &&
  a.from === b.from &&
  a.to === b.to &&
  a.lane === b.lane &&
  a.foldedAt === b.foldedAt;

/**
 * Fold snapshot `d` into `w` at time `now`: progress since the last snapshot is
 * credited to the lane on each piece, and a lane that has closed hands its
 * work to the top row. A restart (fewer pieces, or one that went backwards)
 * starts over. Returns `w` itself when nothing changed.
 */
export function advance(w: LaneWork, d: DownloadSnapshot, now: number): LaneWork {
  const segs = d.segments;
  const restarted =
    segs.length < w.seen.length || segs.some((s, i) => i < w.seen.length && reached(s) < w.seen[i]);
  const base = restarted ? EMPTY_WORK : w;
  const live = liveLanes(d);
  const spans: Span[] = [];
  for (const span of base.spans) {
    const end = segs[span.piece]?.end ?? span.to;
    // A piece's end moves in when another lane takes its back half.
    const next = { ...span, from: Math.min(span.from, end), to: Math.min(span.to, end) };
    if (next.lane !== NO_LANE && !live.includes(next.lane)) {
      next.lane = NO_LANE;
      next.foldedAt = now;
    }
    spans.push(next);
  }
  // Bytes already there at the first snapshot were written before this app
  // watched; a piece that appears later (a lane took over a back half) is
  // credited from its start.
  const watching = base.seen.length > 0;
  segs.forEach((s, i) => {
    const at = reached(s);
    const known = i < base.seen.length;
    if (!watching && at > s.start) spans.push({ piece: i, from: s.start, to: at, lane: NO_LANE });
    const from = known ? Math.min(base.seen[i], s.end) : watching ? s.start : at;
    const lane = d.lanes?.[i] ?? null;
    let last: Span | undefined;
    for (let k = spans.length - 1; k >= 0; k--) {
      if (spans[k].piece === i) {
        last = spans[k];
        break;
      }
    }
    // Progress on a piece nobody holds now came from whoever was on it.
    const writer = lane ?? (last && last.to === from ? last.lane : NO_LANE);
    if (lane == null && at <= from) return;
    if (last && last.lane === writer && last.to === from && last.foldedAt === undefined) {
      last.to = at;
    } else if (at > from || lane != null) {
      spans.push({ piece: i, from, to: at, lane: writer });
    }
  });
  const next: LaneWork = {
    spans,
    seen: segs.map(reached),
    peak: Math.max(base.peak, live.length),
  };
  const same =
    next.peak === w.peak &&
    next.seen.length === w.seen.length &&
    next.seen.every((v, i) => v === w.seen[i]) &&
    next.spans.length === w.spans.length &&
    next.spans.every((s, i) => sameSpan(s, w.spans[i]));
  return same ? w : next;
}

/** Every download's work after the latest snapshots; downloads that are gone drop out. */
export function advanceAll(
  prev: ReadonlyMap<number, LaneWork>,
  ds: readonly DownloadSnapshot[],
  now: number,
): Map<number, LaneWork> {
  const next = new Map<number, LaneWork>();
  for (const d of ds) next.set(d.id, advance(prev.get(d.id) ?? EMPTY_WORK, d, now));
  return next;
}

/** A stretch of the file as a share of it, for drawing: `left` and `width` in %. */
export interface Bar {
  key: string;
  left: number;
  width: number;
  /** Work that just moved here from a lane that closed. */
  fresh?: boolean;
}

/** One lane's row, or the single row a download shows with no lane live. */
export interface RowView {
  /** The lane's number; null for the single row. */
  lane: number | null;
  /** The piece it is on now, from where this lane started on it to where it has got. */
  now: Bar | null;
  /** Work it finished earlier (and, on the top row, closed lanes' work). */
  own: Bar[];
  /** Unfinished work handed back and waiting for a lane (top row only). */
  gaps: Bar[];
  /** The piece it is on, for the tooltip. */
  piece: number | null;
}

const bar = (key: string, from: number, to: number, total: number, fresh?: boolean): Bar => {
  const t = Math.max(1, total);
  return {
    key,
    left: (from / t) * 100,
    width: (Math.max(0, to - from) / t) * 100,
    ...(fresh ? { fresh } : {}),
  };
};

/** `bars` left to right, with touching bars of the same look drawn as one. */
function joined(bars: Bar[]): Bar[] {
  const out: Bar[] = [];
  for (const b of [...bars].sort((x, y) => x.left - y.left)) {
    const last = out[out.length - 1];
    if (last && last.fresh === b.fresh && Math.abs(last.left + last.width - b.left) < 1e-9) {
      out[out.length - 1] = { ...last, width: b.left + b.width - last.left };
    } else out.push(b);
  }
  return out;
}

/** Pieces started, then handed back with nobody on them. */
function waitingGaps(d: DownloadSnapshot, total: number): Bar[] {
  return d.segments.flatMap((s, i) =>
    (d.lanes?.[i] ?? null) == null && s.done > 0 && !finished(s)
      ? [bar(`gap-${i}`, reached(s), s.end, total)]
      : [],
  );
}

/**
 * The rows to draw: one per live lane, lowest first, the top one also holding
 * closed lanes' work and the waiting gaps; with no lane live, one row with all
 * the work so far. Spans of zero width are dropped.
 */
export function rowViews(d: DownloadSnapshot, w: LaneWork, now: number): RowView[] {
  const total = d.total ?? 0;
  const live = liveLanes(d);
  if (live.length === 0) {
    const own = d.segments
      .map((s, i) => bar(`own-${i}`, s.start, reached(s), total))
      .filter((b) => b.width > 0);
    return [{ lane: null, now: null, own, gaps: waitingGaps(d, total), piece: null }];
  }
  return live.map((lane, row) => {
    const piece = (d.lanes ?? []).indexOf(lane);
    let current: Span | undefined;
    for (let k = w.spans.length - 1; k >= 0 && piece >= 0; k--) {
      const s = w.spans[k];
      if (s.piece === piece && s.lane === lane) {
        current = s;
        break;
      }
    }
    const seg = piece >= 0 ? d.segments[piece] : undefined;
    const nowFrom = current ? current.from : seg ? reached(seg) : 0;
    const nowTo = seg ? reached(seg) : nowFrom;
    const mine = w.spans.filter(
      (s) => s !== current && (s.lane === lane || (row === 0 && s.lane === NO_LANE)),
    );
    const own = joined(
      mine
        .map((s) =>
          bar(
            `${s.piece}-${s.from}`,
            s.from,
            s.to,
            total,
            s.foldedAt !== undefined && now - s.foldedAt < FOLD_MS,
          ),
        )
        .filter((b) => b.width > 0),
    );
    return {
      lane,
      now: seg ? bar(`now-${piece}`, nowFrom, nowTo, total) : null,
      own,
      gaps: row === 0 ? waitingGaps(d, total) : [],
      piece: piece >= 0 ? piece : null,
    };
  });
}

/** The line over the rows, as a bold lead and a quieter rest. */
export function rowsHead(d: DownloadSnapshot, w: LaneWork): { lead: string; rest: string } {
  if (d.status === "completed") {
    // The engine counts every report; the GUI's own count only what it saw.
    const peak = Math.max(d.peak_lanes ?? 0, w.peak);
    const lanes = peak > 1 ? `up to ${peak} lanes` : peak === 1 ? "on 1 lane" : "";
    return { lead: `Downloaded in ${d.segments.length} pieces`, rest: lanes };
  }
  switch (d.status) {
    case "downloading": {
      const k = liveLanes(d).length;
      return { lead: k > 0 ? `${k} lane${k === 1 ? "" : "s"}` : "", rest: "" };
    }
    case "paused":
      return { lead: "Paused", rest: "" };
    case "queued":
      return { lead: "Waiting", rest: "" };
    default:
      return { lead: "Stopped", rest: `${formatBytes(d.downloaded)} kept` };
  }
}

/** Bytes `from`–`to` as one range, the unit said once when both share it: "19.1–23.8 MB". */
export function byteRange(from: number, to: number): string {
  const [a, aUnit] = splitUnit(formatBytes(from));
  const [b, bUnit] = splitUnit(formatBytes(to));
  return aUnit === bUnit ? `${a}–${b} ${bUnit}` : `${a} ${aUnit}–${b} ${bUnit}`;
}

const splitUnit = (text: string): [string, string] => {
  const space = text.lastIndexOf(" ");
  return [text.slice(0, space), text.slice(space + 1)];
};

/** The hover tooltip for lane `lane`'s row, on segment `piece`: "Lane 3 · piece 5 · 19.1–23.8 MB · 40% downloaded". */
export function rowTip(d: DownloadSnapshot, lane: number, piece: number): string {
  const s = d.segments[piece];
  const pct = Math.floor((Math.min(s.done, pieceLen(s)) / Math.max(1, pieceLen(s))) * 100);
  return `Lane ${lane} · piece ${piece + 1} · ${byteRange(s.start, s.end)} · ${pct}% downloaded`;
}
