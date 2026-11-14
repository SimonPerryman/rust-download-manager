import { describe, expect, it } from "vitest";
import {
  advance,
  advanceAll,
  byteRange,
  EMPTY_WORK,
  FOLD_MS,
  liveLanes,
  NO_LANE,
  rowsHead,
  rowTip,
  rowViews,
  type Bar,
  type LaneWork,
  type RowView,
} from "./laneWork";
import { snap } from "./testSnapshot";
import type { DownloadSnapshot, Status } from "./types";

/** A 400-byte download in the given pieces (`[start, end, done]`) and lanes. */
const at = (
  pieces: [number, number, number][],
  lanes: (number | null)[],
  status: Status = "downloading",
): DownloadSnapshot =>
  snap({
    status,
    total: 400,
    downloaded: pieces.reduce((n, [, , done]) => n + done, 0),
    segments: pieces.map(([start, end, done]) => ({ start, end, done })),
    lanes,
  });

/** Folds snapshots in order, a second apart. */
const play = (...ds: DownloadSnapshot[]): LaneWork =>
  ds.reduce((w, d, i) => advance(w, d, i * 1000), EMPTY_WORK);

/** A bar as its `[from, to]` bytes over the 400-byte file. */
const span = (b: Bar) => [Math.round(b.left * 4), Math.round((b.left + b.width) * 4)];

/** Each row's bars as byte pairs. */
const bytes = (row: RowView) => ({
  lane: row.lane,
  now: row.now ? span(row.now) : null,
  own: row.own.map(span),
  gaps: row.gaps.map(span),
});

/** Every drawn bar across `rows`, so a test can check none overlap. */
const drawn = (rows: RowView[]) =>
  rows
    .flatMap((r) => [...r.own, ...(r.now ? [r.now] : [])])
    .map(span)
    .sort((a, b) => a[0] - b[0]);

describe("liveLanes", () => {
  it("lists the lanes on a piece, lowest first, only while downloading", () => {
    expect(
      liveLanes(
        at(
          [
            [0, 200, 5],
            [200, 400, 5],
          ],
          [2, 1],
        ),
      ),
    ).toEqual([1, 2]);
    expect(
      liveLanes(
        at(
          [
            [0, 200, 5],
            [200, 400, 5],
          ],
          [2, 1],
          "paused",
        ),
      ),
    ).toEqual([]);
    expect(
      liveLanes(
        at(
          [
            [0, 200, 5],
            [200, 400, 5],
          ],
          [null, null],
        ),
      ),
    ).toEqual([]);
  });
});

describe("rowViews", () => {
  it("gives each lane its own row with only its own work, so no byte is drawn twice", () => {
    const four = (d1: number, d2: number): DownloadSnapshot =>
      at(
        [
          [0, 100, d1],
          [100, 200, d2],
          [200, 300, 0],
          [300, 400, 0],
        ],
        [1, 2, null, null],
      );
    const d = four(60, 20);
    const w = play(four(0, 0), four(30, 10), d);
    const rows = rowViews(d, w, 2000).map(bytes);
    expect(rows).toEqual([
      { lane: 1, now: [0, 60], own: [], gaps: [] },
      { lane: 2, now: [100, 120], own: [], gaps: [] },
    ]);
  });

  it("keeps a finished piece on its lane's row, softer, as the lane moves on", () => {
    const before = at(
      [
        [0, 100, 90],
        [100, 200, 0],
      ],
      [1, null],
    );
    const after = at(
      [
        [0, 100, 100],
        [100, 200, 10],
      ],
      [null, 1],
    );
    const start = at(
      [
        [0, 100, 0],
        [100, 200, 0],
      ],
      [1, null],
    );
    const rows = rowViews(after, play(start, before, after), 2000).map(bytes);
    expect(rows).toEqual([{ lane: 1, now: [100, 110], own: [[0, 100]], gaps: [] }]);
  });

  it("moves a closed lane's work up to Lane 1's row and marks its unfinished rest", () => {
    const three = at(
      [
        [0, 100, 50],
        [100, 200, 40],
        [200, 300, 20],
      ],
      [1, 2, 3],
    );
    const shed = at(
      [
        [0, 100, 55],
        [100, 200, 45],
        [200, 300, 20],
      ],
      [1, 2, null],
    );
    const w = play(
      at(
        [
          [0, 100, 0],
          [100, 200, 0],
          [200, 300, 0],
        ],
        [1, 2, 3],
      ),
      three,
      shed,
    );
    const rows = rowViews(shed, w, 2000);
    expect(rows.map(bytes)).toEqual([
      { lane: 1, now: [0, 55], own: [[200, 220]], gaps: [[220, 300]] },
      { lane: 2, now: [100, 145], own: [], gaps: [] },
    ]);
    expect(rows[0].own[0].fresh).toBe(true);
    expect(drawn(rows)).toEqual([
      [0, 55],
      [100, 145],
      [200, 220],
    ]);
    // The flash is over once FOLD_MS has passed.
    expect(rowViews(shed, w, 2000 + FOLD_MS).map((r) => r.own.map((b) => b.fresh))).toEqual([
      [undefined],
      [],
    ]);
  });

  it("draws a taken-back piece from where the new lane picked it up", () => {
    const handed = at(
      [
        [0, 100, 100],
        [100, 200, 40],
      ],
      [1, null],
    );
    const taken = at(
      [
        [0, 100, 100],
        [100, 200, 50],
      ],
      [null, 1],
    );
    const both = (d1: number, d2: number) =>
      at(
        [
          [0, 100, d1],
          [100, 200, d2],
        ],
        [1, 2],
      );
    const w = play(both(0, 0), both(90, 40), handed, taken);
    // Lane 2's 40 bytes stay soft on the top row; Lane 1's strong fill starts at 140.
    const rows = rowViews(taken, w, 3000);
    expect(rows.map(bytes)).toEqual([
      {
        lane: 1,
        now: [140, 150],
        own: [
          [0, 100],
          [100, 140],
        ],
        gaps: [],
      },
    ]);
    expect(rows[0].own.map((b) => b.fresh)).toEqual([undefined, true]);
    // Once the flash is over, touching work on a row is drawn as one bar.
    expect(rowViews(taken, w, 2000 + FOLD_MS).map(bytes)[0].own).toEqual([[0, 140]]);
  });

  it("splits a piece when a lane takes over its back half", () => {
    const before = at(
      [
        [0, 200, 100],
        [200, 400, 200],
      ],
      [1, 2],
    );
    // Lane 2 finished piece 2 and took 150..200, the back half of what Lane 1 had left.
    const after = at(
      [
        [0, 150, 110],
        [200, 400, 200],
        [150, 200, 5],
      ],
      [1, null, 2],
    );
    const start = at(
      [
        [0, 200, 0],
        [200, 400, 0],
      ],
      [1, 2],
    );
    const rows = rowViews(after, play(start, before, after), 2000).map(bytes);
    expect(rows).toEqual([
      { lane: 1, now: [0, 110], own: [], gaps: [] },
      { lane: 2, now: [150, 155], own: [[200, 400]], gaps: [] },
    ]);
  });

  it("puts bytes from before the app was watching on the top row", () => {
    const d = at(
      [
        [0, 200, 120],
        [200, 400, 0],
      ],
      [1, 2],
    );
    const w = advance(EMPTY_WORK, d, 0);
    expect(w.spans.find((s) => s.lane === NO_LANE)).toMatchObject({ from: 0, to: 120 });
    expect(rowViews(d, w, 0).map(bytes)).toEqual([
      { lane: 1, now: [120, 120], own: [[0, 120]], gaps: [] },
      { lane: 2, now: [200, 200], own: [], gaps: [] },
    ]);
  });

  it("shows one row with all the work and the waiting gaps when no lane is live", () => {
    const d = at(
      [
        [0, 200, 200],
        [200, 400, 50],
      ],
      [],
      "paused",
    );
    expect(rowViews(d, EMPTY_WORK, 0).map(bytes)).toEqual([
      {
        lane: null,
        now: null,
        own: [
          [0, 200],
          [200, 250],
        ],
        gaps: [[250, 400]],
      },
    ]);
  });
});

describe("advance", () => {
  it("returns the same record when nothing moved", () => {
    const d = at([[0, 400, 10]], [1]);
    const w = advance(EMPTY_WORK, d, 0);
    expect(advance(w, d, 500)).toBe(w);
  });

  it("starts over when the download restarts from zero", () => {
    const w = play(at([[0, 400, 300]], [1]), at([[0, 400, 0]], [1]));
    expect(w.spans.map((s) => [s.from, s.to])).toEqual([[0, 0]]);
  });

  it("remembers the most lanes live at once", () => {
    const two = at(
      [
        [0, 200, 10],
        [200, 400, 10],
      ],
      [1, 2],
    );
    const one = at(
      [
        [0, 200, 20],
        [200, 400, 10],
      ],
      [1, null],
    );
    expect(play(two, one).peak).toBe(2);
  });

  it("drops downloads that are gone", () => {
    const d = at([[0, 400, 10]], [1]);
    const all = advanceAll(new Map([[99, EMPTY_WORK]]), [d], 0);
    expect([...all.keys()]).toEqual([d.id]);
  });
});

describe("rowsHead", () => {
  const pieces: [number, number, number][] = [
    [0, 200, 200],
    [200, 400, 50],
  ];

  it("counts the live lanes, and nothing else", () => {
    expect(rowsHead(at(pieces, [null, 1]), EMPTY_WORK)).toEqual({ lead: "1 lane", rest: "" });
    expect(rowsHead(at(pieces, [1, 2]), EMPTY_WORK).lead).toBe("2 lanes");
    expect(rowsHead(at(pieces, [null, null]), EMPTY_WORK)).toEqual({ lead: "", rest: "" });
  });

  it("names the state when no lane is live, and what's kept when stopped", () => {
    expect(rowsHead(at(pieces, [], "paused"), EMPTY_WORK)).toEqual({ lead: "Paused", rest: "" });
    expect(rowsHead(at(pieces, [], "queued"), EMPTY_WORK)).toEqual({ lead: "Queued", rest: "" });
    expect(rowsHead(at(pieces, [], "failed"), EMPTY_WORK)).toEqual({
      lead: "Stopped",
      rest: "250 B kept",
    });
    expect(rowsHead(at(pieces, [], "cancelled"), EMPTY_WORK).lead).toBe("Stopped");
  });

  it("sums up a completed download with the most lanes it used", () => {
    const done = at(
      [
        [0, 200, 200],
        [200, 400, 200],
      ],
      [],
      "completed",
    );
    expect(rowsHead(done, { ...EMPTY_WORK, peak: 3 })).toEqual({
      lead: "Downloaded in 2 pieces",
      rest: "up to 3 lanes",
    });
    expect(rowsHead(done, { ...EMPTY_WORK, peak: 1 }).rest).toBe("on 1 lane");
    expect(rowsHead(done, EMPTY_WORK).rest).toBe("");
    // The engine's count wins when the snapshots missed the busiest moment.
    expect(rowsHead({ ...done, peak_lanes: 4 }, { ...EMPTY_WORK, peak: 2 }).rest).toBe(
      "up to 4 lanes",
    );
  });
});

describe("byteRange", () => {
  it("says a shared unit once", () => {
    const MB = 1024 * 1024;
    expect(byteRange(19.1 * MB, 23.8 * MB)).toBe("19.1–23.8 MB");
    expect(byteRange(0, 2 * MB)).toBe("0 B–2.00 MB");
    expect(byteRange(100, 400)).toBe("100–400 B");
  });
});

describe("rowTip", () => {
  it("names the lane, its piece, where the piece sits and how far it has got", () => {
    const d = at(
      [
        [0, 200, 200],
        [200, 400, 50],
      ],
      [null, 2],
    );
    expect(rowTip(d, 2, 1)).toBe("Lane 2 · piece 2 · 200–400 B · 25% downloaded");
  });
});
