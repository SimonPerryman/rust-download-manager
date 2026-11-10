import { describe, expect, it } from "vitest";
import {
  archivedMessage,
  finishedToArchive,
  isArchivable,
  restoredMessage,
  toRemove,
} from "./archive";
import { snap } from "./testSnapshot";

describe("finishedToArchive", () => {
  it("takes done and stopped rows not archived yet", () => {
    const ds = [
      snap({ id: 1, status: "completed" }),
      snap({ id: 2, status: "cancelled", partial_kept: true }),
      snap({ id: 3, status: "failed" }),
      snap({ id: 4, status: "downloading" }),
      snap({ id: 5, status: "completed", archived: true }),
    ];
    expect(finishedToArchive(ds)).toEqual([1, 2]);
  });
});

describe("isArchivable", () => {
  it("is any finished row outside the archive", () => {
    expect(isArchivable(snap({ status: "failed" }))).toBe(true);
    expect(isArchivable(snap({ status: "paused" }))).toBe(false);
    expect(isArchivable(snap({ status: "completed", archived: true }))).toBe(false);
  });
});

describe("toRemove", () => {
  it("keeps only archived, finished rows, in order", () => {
    const ds = [
      snap({ id: 1, status: "completed", archived: true }),
      snap({ id: 2, status: "completed" }),
      snap({ id: 3, status: "queued", archived: true }),
      snap({ id: 4, status: "failed", archived: true }),
      snap({ id: 5, status: "cancelled", archived: true }),
    ];
    expect(toRemove(ds).map((d) => d.id)).toEqual([1, 4, 5]);
  });

  it("drops a row unarchived or resumed since it was picked", () => {
    const picked = snap({ id: 1, status: "completed", archived: true });
    expect(toRemove([{ ...picked, archived: false }])).toEqual([]);
    const failed = snap({ id: 2, status: "failed", archived: true });
    expect(toRemove([{ ...failed, status: "downloading" }])).toEqual([]);
  });
});

describe("toast text", () => {
  it("names one download, counts several", () => {
    expect(archivedMessage([snap({ filename: "a.iso" })])).toBe("Archived a.iso.");
    expect(archivedMessage([snap({ id: 1 }), snap({ id: 2 })])).toBe("Archived 2 downloads.");
    expect(restoredMessage(1)).toBe("Restored");
    expect(restoredMessage(2)).toBe("Restored 2 downloads");
  });
});
