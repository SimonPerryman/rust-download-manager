import { describe, expect, it } from "vitest";
import {
  CLEAR_STATUSES,
  clearMessage,
  INITIAL_CLEAR,
  planBatch,
  reduceClear,
  restoredMessage,
  type ClearEffect,
  type ClearEvent,
  type ClearState,
} from "./pendingClear";
import { snap } from "./testSnapshot";
import type { DownloadSnapshot } from "./types";

const done = (id: number) => snap({ id, status: "completed" });
const stopped = (id: number, kept = false) => snap({ id, status: "cancelled", partial_kept: kept });

/** Applies events in order from the initial state; returns the final state and every effect. */
function play(...events: ClearEvent[]) {
  let state: ClearState = INITIAL_CLEAR;
  const effects: ClearEffect[] = [];
  for (const e of events) {
    const r = reduceClear(state, e);
    state = r.state;
    effects.push(...r.effects);
  }
  return { state, effects };
}

const sorted = (s: ReadonlySet<number>) => [...s].sort((a, b) => a - b);

describe("planBatch", () => {
  it("captures exactly the done and stopped ids, counting kept partials", () => {
    const ds: DownloadSnapshot[] = [
      done(1),
      stopped(2, true),
      stopped(3),
      snap({ id: 4, status: "failed" }),
      snap({ id: 5, status: "downloading" }),
      snap({ id: 6, status: "queued" }),
      snap({ id: 7, status: "paused" }),
    ];
    expect(planBatch(ds, 9)).toEqual({
      seq: 9,
      ids: [1, 2, 3],
      kept: 1,
      message: "Cleared 3 finished downloads. Files and kept partial files stay on disk.",
    });
  });

  it("is null when nothing is finished", () => {
    expect(planBatch([snap({ id: 1, status: "failed" })], 0)).toBeNull();
    expect(planBatch([], 0)).toBeNull();
  });

  it("clears the statuses the engine calls done and stopped", () => {
    expect(CLEAR_STATUSES).toEqual(["completed", "cancelled"]);
  });
});

describe("reduceClear", () => {
  it("a click with nothing to clear changes nothing", () => {
    const r = reduceClear(INITIAL_CLEAR, { type: "click", downloads: [snap({ id: 1 })] });
    expect(r.state).toBe(INITIAL_CLEAR);
    expect(r.effects).toEqual([]);
  });

  it("a click hides the captured rows and arms the timer, committing nothing yet", () => {
    const { state, effects } = play({ type: "click", downloads: [done(1), stopped(2)] });
    expect(state.pending?.ids).toEqual([1, 2]);
    expect(sorted(state.hidden)).toEqual([1, 2]);
    expect(effects).toEqual([{ type: "arm", seq: state.pending!.seq }]);
  });

  it("undo restores the rows and disarms", () => {
    const { state, effects } = play({ type: "click", downloads: [done(1)] }, { type: "undo" });
    expect(state.pending).toBeNull();
    expect(state.hidden.size).toBe(0);
    expect(effects[effects.length - 1]).toEqual({ type: "disarm" });
  });

  it("undo with nothing pending is a no-op", () => {
    const r = reduceClear(INITIAL_CLEAR, { type: "undo" });
    expect(r.state).toBe(INITIAL_CLEAR);
    expect(r.effects).toEqual([]);
  });

  it("the timeout commits only the ids captured at click time", () => {
    // Row 2 completes inside the undo window: it isn't in the batch, so it's spared.
    const first = play({
      type: "click",
      downloads: [done(1), snap({ id: 2, status: "downloading" })],
    });
    const seq = first.state.pending!.seq;
    const r = reduceClear(first.state, { type: "snapshot", downloads: [done(1), done(2)] });
    const t = reduceClear(r.state, { type: "timeout", seq });
    expect(t.effects).toEqual([
      { type: "commit", batch: expect.objectContaining({ seq, ids: [1] }) },
    ]);
    expect(t.state.pending).toBeNull();
    expect(t.state.committing.map((b) => b.ids)).toEqual([[1]]);
    expect(sorted(t.state.hidden)).toEqual([1]);
  });

  it("ignores a stale timeout", () => {
    const first = play({ type: "click", downloads: [done(1)] }, { type: "undo" });
    const again = reduceClear(first.state, { type: "click", downloads: [done(1)] });
    const staleSeq = again.state.pending!.seq - 1;
    const r = reduceClear(again.state, { type: "timeout", seq: staleSeq });
    expect(r.state).toBe(again.state);
    expect(r.effects).toEqual([]);
  });

  it("a second click commits the pending batch, then makes the new one pending", () => {
    const first = play({ type: "click", downloads: [done(1)] });
    const firstBatch = first.state.pending!;
    const r = reduceClear(first.state, { type: "click", downloads: [done(2)] });
    expect(r.effects).toEqual([
      { type: "commit", batch: firstBatch },
      { type: "arm", seq: r.state.pending!.seq },
    ]);
    expect(r.state.pending!.seq).not.toBe(firstBatch.seq);
    expect(r.state.pending!.ids).toEqual([2]);
    expect(r.state.committing).toEqual([firstBatch]);
    expect(sorted(r.state.hidden)).toEqual([1, 2]);
  });

  it("keeps committed rows hidden until a snapshot without them arrives", () => {
    const first = play({ type: "click", downloads: [done(1), done(2)] });
    const committed = reduceClear(first.state, { type: "timeout", seq: first.state.pending!.seq });
    // The engine hasn't applied the clear yet: rows still present, still hidden.
    const still = reduceClear(committed.state, { type: "snapshot", downloads: [done(1), done(2)] });
    expect(still.state).toBe(committed.state);
    expect(sorted(still.state.hidden)).toEqual([1, 2]);
    // Row 1 dropped first, then row 2: the batch shrinks, then disappears.
    const half = reduceClear(still.state, { type: "snapshot", downloads: [done(2)] });
    expect(sorted(half.state.hidden)).toEqual([2]);
    const gone = reduceClear(half.state, { type: "snapshot", downloads: [] });
    expect(gone.state.committing).toEqual([]);
    expect(gone.state.hidden.size).toBe(0);
  });

  it("a snapshot leaves pending ids alone", () => {
    const first = play({ type: "click", downloads: [done(1)] });
    const r = reduceClear(first.state, { type: "snapshot", downloads: [] });
    expect(r.state.pending?.ids).toEqual([1]);
    expect(sorted(r.state.hidden)).toEqual([1]);
  });

  it("a failed commit unhides that batch's rows", () => {
    const first = play({ type: "click", downloads: [done(1)] });
    const seq = first.state.pending!.seq;
    const committed = reduceClear(first.state, { type: "timeout", seq });
    const r = reduceClear(committed.state, { type: "failed", seq });
    expect(r.state.committing).toEqual([]);
    expect(r.state.hidden.size).toBe(0);
  });

  it("undo after the commit was sent is impossible", () => {
    const first = play({ type: "click", downloads: [done(1)] });
    const committed = reduceClear(first.state, { type: "timeout", seq: first.state.pending!.seq });
    const r = reduceClear(committed.state, { type: "undo" });
    expect(r.state).toBe(committed.state);
    expect(r.effects).toEqual([]);
  });
});

describe("messages", () => {
  it("clearMessage pluralises and mentions kept partials", () => {
    expect(clearMessage(1, 0)).toBe("Cleared 1 finished download. Files stay on disk.");
    expect(clearMessage(3, 0)).toBe("Cleared 3 finished downloads. Files stay on disk.");
    expect(clearMessage(3, 2)).toBe(
      "Cleared 3 finished downloads. Files and kept partial files stay on disk.",
    );
  });

  it("restoredMessage", () => {
    expect(restoredMessage(1)).toBe("Restored");
    expect(restoredMessage(3)).toBe("Restored 3 downloads");
  });
});
