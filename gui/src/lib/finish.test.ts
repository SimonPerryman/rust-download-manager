import { describe, expect, it } from "vitest";
import { parseIdList, pruneUnopened, trackFinishes, type StatusMemo } from "./finish";
import { snap } from "./testSnapshot";
import type { Status } from "./types";

const rows = (...pairs: [number, Status][]) => pairs.map(([id, status]) => snap({ id, status }));

/** Feeds payloads in order from a fresh start; returns the ids each one finished. */
function run(...payloads: ReturnType<typeof rows>[]): number[][] {
  let memo: StatusMemo | null = null;
  return payloads.map((ds) => {
    const r = trackFinishes(memo, ds);
    memo = r.memo;
    return r.finished;
  });
}

describe("trackFinishes", () => {
  it("seeds from the first payload without firing, even for completed rows", () => {
    expect(run(rows([1, "completed"], [2, "downloading"]))).toEqual([[]]);
  });

  it("fires when a downloading or queued row completes", () => {
    expect(
      run(rows([1, "downloading"], [2, "queued"]), rows([1, "completed"], [2, "completed"])),
    ).toEqual([[], [1, 2]]);
  });

  it("fires for a row absent before and already completed (its queued snapshot coalesced away)", () => {
    expect(run(rows([1, "completed"]), rows([1, "completed"], [2, "completed"]))).toEqual([
      [],
      [2],
    ]);
  });

  it("doesn't fire again for a row that stays completed", () => {
    expect(run(rows([1, "downloading"]), rows([1, "completed"]), rows([1, "completed"]))).toEqual([
      [],
      [1],
      [],
    ]);
  });

  it("fires once for failed → queued → completed", () => {
    expect(
      run(rows([1, "failed"]), rows([1, "queued"]), rows([1, "completed"]), rows([1, "completed"])),
    ).toEqual([[], [], [1], []]);
  });

  it("drops removed ids from the memo", () => {
    const { memo } = trackFinishes(
      new Map([
        [1, "completed"],
        [2, "queued"],
      ]),
      rows([2, "queued"]),
    );
    expect([...memo.keys()]).toEqual([2]);
  });
});

describe("pruneUnopened", () => {
  it("keeps only ids still present and completed", () => {
    const ds = rows([1, "completed"], [2, "queued"], [3, "completed"]);
    expect(pruneUnopened(new Set([1, 2, 4]), ds)).toEqual(new Set([1]));
  });
});

describe("parseIdList", () => {
  it("reads a JSON list of integer ids", () => {
    expect(parseIdList("[3,1,2]")).toEqual([3, 1, 2]);
  });

  it("tolerates garbage: bad JSON or a non-list is empty, non-integers are dropped", () => {
    for (const raw of [null, "", "nope", "{}", "3", '"[1]"', "null"]) {
      expect(parseIdList(raw)).toEqual([]);
    }
    expect(parseIdList('[1, "2", 2.5, null, -1, 4]')).toEqual([1, 4]);
  });
});
