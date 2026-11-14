import { beforeEach, describe, expect, it, vi } from "vitest";
import { snap } from "./testSnapshot";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const { acceptOffer, locateFolderFor, locateOne } = await import("./locate");
const { locateFolderMessage, relinkedMessage, sameFolderOfferLine } = await import("./copy");

const gone = (id: number, final_path = "/old/a.zip") =>
  snap({ id, status: "completed", file_missing: true, final_path });
const offer = { count: 2, old_folder: "/old", new_folder: "/new" };

describe("locateOne", () => {
  beforeEach(() => invoke.mockReset());

  it("toasts and passes on the offer after a relink", async () => {
    invoke.mockResolvedValue({ relinked: true, new_path: "/new/a.zip", offer });
    const r = await locateOne(gone(1));
    expect(r.message).toBe("Relinked a.zip (now in new)");
    expect(r.offer).toEqual(offer);
    expect(invoke.mock.calls).toEqual([["locate_download", { id: 1 }]]);
  });

  it("shows nothing when there is nothing else to offer", async () => {
    invoke.mockResolvedValue({ relinked: true, new_path: "/new/a.zip", offer: null });
    expect((await locateOne(gone(1))).offer).toBeNull();
  });

  it("shows nothing when the picker was cancelled", async () => {
    invoke.mockResolvedValue({ relinked: false, new_path: null, offer: null });
    expect(await locateOne(gone(1))).toEqual({ message: null, offer: null });
  });
});

describe("accepting the offer", () => {
  beforeEach(() => invoke.mockReset());

  it("names no ids or paths, and reports how many were relinked", async () => {
    invoke.mockResolvedValue(2);
    expect(await acceptOffer()).toBe("Relinked 2 more downloads.");
    expect(invoke.mock.calls).toEqual([["accept_same_folder_offer"]]);
  });
});

describe("locateFolderFor", () => {
  beforeEach(() => invoke.mockReset());

  it("sends only the missing rows' ids and reports matched and not matched", async () => {
    invoke.mockResolvedValue({ relinked: 2, not_found: [4] });
    const rows = [gone(1), gone(2), snap({ id: 3, status: "completed" }), gone(4)];
    expect(await locateFolderFor(rows)).toEqual({
      message: "Relinked 2. 1 wasn't found in that folder.",
      notFoundIds: [4],
    });
    expect(invoke.mock.calls).toEqual([["locate_folder", { ids: [1, 2, 4] }]]);
  });

  it("asks nothing when no row is missing, and stays quiet when cancelled", async () => {
    expect(await locateFolderFor([snap({ status: "completed" })])).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
    invoke.mockResolvedValue(null);
    expect(await locateFolderFor([gone(1), gone(2)])).toBeNull();
  });

  it("offers Show not found only when some were found and some were not", async () => {
    invoke.mockResolvedValue({ relinked: 0, not_found: [1, 2] });
    expect((await locateFolderFor([gone(1), gone(2)]))?.notFoundIds).toEqual([]);
    invoke.mockResolvedValue({ relinked: 2, not_found: [] });
    expect((await locateFolderFor([gone(1), gone(2)]))?.notFoundIds).toEqual([]);
    invoke.mockResolvedValue({ relinked: 1, not_found: [2] });
    expect((await locateFolderFor([gone(1), gone(2)]))?.notFoundIds).toEqual([2]);
  });
});

describe("copy", () => {
  it("words the offer and the folder result for one and many", () => {
    expect(sameFolderOfferLine(1)).toMatch(/^1 other missing download from/);
    expect(sameFolderOfferLine(3)).toMatch(/^3 other missing downloads from/);
    expect(locateFolderMessage(0, 3)).toBe("None of the 3 downloads' files are in that folder.");
    expect(locateFolderMessage(3, 0)).toBe("Relinked 3 downloads.");
    expect(locateFolderMessage(1, 0)).toBe("Relinked 1 download.");
    expect(locateFolderMessage(2, 3)).toBe("Relinked 2. 3 weren't found in that folder.");
    expect(locateFolderMessage(1, 1)).toBe("Relinked 1. 1 wasn't found in that folder.");
    expect(locateFolderMessage(0, 1)).toBe("That download's file isn't in that folder.");
  });

  it("words a relink by what changed: the folder if the name held, else the name", () => {
    expect(relinkedMessage(String.raw`C:\Users\me\Downloads\a.zip`, String.raw`D:\Backup\Old\a.zip`)).toBe(
      "Relinked a.zip (now in Old)",
    );
    expect(relinkedMessage("/old/a.zip", "/new/b-copy.zip")).toBe(
      "Relinked a.zip, now named b-copy.zip",
    );
  });
});
