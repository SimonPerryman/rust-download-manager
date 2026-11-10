import { beforeEach, describe, expect, it, vi } from "vitest";
import { nothingRestored } from "./copy";
import { snap } from "./testSnapshot";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const { archiveRows, undoArchive } = await import("./useArchive");

describe("archiveRows", () => {
  beforeEach(() => invoke.mockReset());

  // A row finished when picked may have been resumed since; Undo must not touch it.
  it("resolves with the ids the engine archived, not the ids asked for", async () => {
    invoke.mockResolvedValue([2]);
    const rows = [
      snap({ id: 1, status: "completed" }),
      snap({ id: 2, status: "failed" }),
      snap({ id: 3, status: "downloading" }),
    ];
    expect(await archiveRows(rows)).toEqual([2]);
    expect(invoke.mock.calls).toEqual([["archive", { ids: [1, 2] }]]);
  });

  it("asks nothing when no row is archivable", async () => {
    expect(await archiveRows([snap({ status: "paused" })])).toEqual([]);
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("undoArchive", () => {
  beforeEach(() => invoke.mockReset());
  const batch = { seq: 1, ids: [1, 2, 3], message: "Archived 3 downloads." };

  it("counts the downloads the engine restored", async () => {
    invoke.mockResolvedValue(2);
    expect(await undoArchive(batch)).toBe("Restored 2 downloads");
    expect(invoke.mock.calls).toEqual([["unarchive", { ids: [1, 2, 3] }]]);
  });

  it("claims nothing when none were still archived", async () => {
    invoke.mockResolvedValue(0);
    expect(await undoArchive(batch)).toBe(nothingRestored);
  });
});
