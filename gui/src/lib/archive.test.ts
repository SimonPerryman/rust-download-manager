import { describe, expect, it } from "vitest";
import { archivedMessage, finishedToArchive, isArchivable, restoredMessage } from "./archive";
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

describe("toast text", () => {
  it("names one download, counts several", () => {
    expect(archivedMessage([snap({ filename: "a.iso" })])).toBe("Archived a.iso.");
    expect(archivedMessage([snap({ id: 1 }), snap({ id: 2 })])).toBe("Archived 2 downloads.");
    expect(restoredMessage(1)).toBe("Restored");
    expect(restoredMessage(2)).toBe("Restored 2 downloads");
  });
});
