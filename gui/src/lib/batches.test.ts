import { describe, expect, it } from "vitest";
import {
  batchGroups,
  batchNameFromStem,
  failedText,
  folderName,
  folderTickLabel,
  joinTickLabel,
  layoutRows,
  UNGROUPED,
  ungroupedText,
  retryBatchMessage,
  rollupText,
} from "./batches";
import { snap } from "./testSnapshot";
import type { DownloadSnapshot } from "./types";

/** A file of batch `batch` (named "Course" or "Photos"). */
const inBatch = (id: number, batch: number, over: Partial<DownloadSnapshot> = {}) =>
  snap({ id, batch, batch_label: batch === 1 ? "Course" : "Photos", ...over });

describe("batchGroups", () => {
  it("rolls up each batch's working files", () => {
    const ds = [
      inBatch(1, 1, { status: "completed", downloaded: 100, total: 100 }),
      inBatch(2, 1, { status: "failed" }),
      inBatch(3, 1, { status: "downloading", downloaded: 50, total: 100 }),
      inBatch(4, 1, { status: "queued" }),
      snap({ id: 5 }),
    ];
    const [g] = batchGroups(ds);
    expect(batchGroups(ds)).toHaveLength(1);
    expect(g).toMatchObject({ id: 1, label: "Course", done: 1, failed: 1 });
    expect(g.items.map((d) => d.id)).toEqual([1, 2, 3, 4]);
    expect(g.fraction).toBeCloseTo((1 + 0 + 0.5 + 0) / 4);
    expect(rollupText(g)).toBe("1 of 4 done");
    expect(failedText(g)).toBe("1 failed");
  });

  it("leaves archived files out, and a batch with none left", () => {
    const ds = [
      inBatch(1, 1, { status: "completed", archived: true }),
      inBatch(2, 2, { status: "completed" }),
      inBatch(3, 2, { status: "completed", archived: true }),
    ];
    const groups = batchGroups(ds);
    expect(groups.map((g) => [g.id, g.items.length])).toEqual([[2, 1]]);
  });

  it("lists batches by name", () => {
    const ds = [inBatch(1, 2), inBatch(2, 1)];
    expect(batchGroups(ds).map((g) => g.label)).toEqual(["Course", "Photos"]);
  });
});

describe("layoutRows", () => {
  const rows = [inBatch(1, 1), snap({ id: 2 }), inBatch(3, 1), inBatch(4, 2), inBatch(5, 1)];
  const groups = batchGroups(rows);
  const kinds = (items: ReturnType<typeof layoutRows>["items"]) =>
    items.map((i) =>
      i.kind === "group"
        ? `group:${i.group.id}`
        : i.kind === "ungrouped"
          ? "ungrouped"
          : `row:${i.d.id}`,
    );

  it("is the plain list when not grouped", () => {
    const { items, shown } = layoutRows(rows, groups, false, new Set());
    expect(kinds(items)).toEqual(["row:1", "row:2", "row:3", "row:4", "row:5"]);
    expect(shown.map((d) => d.id)).toEqual([1, 2, 3, 4, 5]);
  });

  it("gathers a batch under one group row, with the rest last under Ungrouped", () => {
    const { items, shown } = layoutRows(rows, groups, true, new Set());
    expect(kinds(items)).toEqual([
      "group:1",
      "row:1",
      "row:3",
      "row:5",
      "group:2",
      "row:4",
      "ungrouped",
      "row:2",
    ]);
    expect(shown.map((d) => d.id)).toEqual([1, 3, 5, 4, 2]);
    expect(items.find((i) => i.kind === "ungrouped")).toMatchObject({ count: 1 });
  });

  it("lists only the group row of a collapsed batch, and selection skips its files", () => {
    const { items, shown } = layoutRows(rows, groups, true, new Set([1]));
    expect(kinds(items)).toEqual(["group:1", "group:2", "row:4", "ungrouped", "row:2"]);
    expect(shown.map((d) => d.id)).toEqual([4, 2]);
  });

  it("collapses Ungrouped like a batch", () => {
    const { items, shown } = layoutRows(rows, groups, true, new Set([UNGROUPED]));
    expect(kinds(items).slice(-2)).toEqual(["row:4", "ungrouped"]);
    expect(items[items.length - 1]).toMatchObject({ collapsed: true });
    expect(shown.map((d) => d.id)).toEqual([1, 3, 5, 4]);
  });

  it("has no Ungrouped header without a batch in view, or without a loose download", () => {
    const only = [inBatch(1, 1), inBatch(2, 1)];
    expect(kinds(layoutRows(only, batchGroups(only), true, new Set()).items)).toEqual([
      "group:1",
      "row:1",
      "row:2",
    ]);
    const loose = [snap({ id: 7 }), snap({ id: 8 })];
    expect(kinds(layoutRows(loose, [], true, new Set()).items)).toEqual(["row:7", "row:8"]);
  });

  it("words the Ungrouped count", () => {
    expect(ungroupedText(1)).toBe("1 download");
    expect(ungroupedText(6)).toBe("6 downloads");
  });

  it("shows a group row over the files a filter left, rolled up over all of them", () => {
    const failed = [inBatch(1, 1, { status: "failed" })];
    const all = [...failed, inBatch(2, 1, { status: "completed" })];
    const { items } = layoutRows(failed, batchGroups(all), true, new Set());
    const group = items[0];
    expect(group.kind === "group" && rollupText(group.group)).toBe("1 of 2 done");
    expect(kinds(items)).toEqual(["group:1", "row:1"]);
  });
});

describe("the Add dialog's batch name and folder", () => {
  it("names a batch from a pattern's stem", () => {
    expect(batchNameFromStem("lesson")).toBe("lesson");
    expect(batchNameFromStem("img_")).toBe("img");
    expect(batchNameFromStem("part-")).toBe("part");
    expect(batchNameFromStem("example.com")).toBe("example.com");
    expect(batchNameFromStem("x".repeat(80))).toHaveLength(64);
  });

  it("words the folder tick with the destination's last part", () => {
    expect(folderName("C:\\Users\\Simon\\Downloads\\")).toBe("Downloads");
    expect(folderName("/home/a/Downloads")).toBe("Downloads");
    expect(folderTickLabel(" Course files ", "C:\\Users\\Simon\\Downloads")).toBe(
      "Create folder “Course files” in Downloads",
    );
    expect(folderTickLabel("A", "")).toBe("Create folder “A” in Downloads");
  });

  it("offers to add to an existing batch", () => {
    expect(joinTickLabel({ id: 1, label: "Course", files: 4 })).toBe("Add to “Course” (4 files)");
    expect(joinTickLabel({ id: 1, label: "Course", files: 1 })).toBe("Add to “Course” (1 file)");
  });

  it("says how many failed files Retry failed started", () => {
    expect(retryBatchMessage(1)).toBe("Retrying 1 failed file");
    expect(retryBatchMessage(3)).toBe("Retrying 3 failed files");
  });
});
