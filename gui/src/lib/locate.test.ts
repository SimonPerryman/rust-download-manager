import { beforeEach, describe, expect, it, vi } from "vitest";
import { snap } from "./testSnapshot";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const { acceptOffer, locateFolderFor, locateOne } = await import("./locate");
const { locateFolderMessage, sameFolderOfferLine } = await import("./copy");

const gone = (id: number) => snap({ id, status: "completed", file_missing: true });
const offer = { count: 2, old_folder: "/old", new_folder: "/new" };

describe("locateOne", () => {
  beforeEach(() => invoke.mockReset());

  it("toasts and passes on the offer after a relink", async () => {
    invoke.mockResolvedValue({ relinked: true, offer });
    const r = await locateOne(gone(1));
    expect(r.message).toMatch(/^Relinked /);
    expect(r.offer).toEqual(offer);
    expect(invoke.mock.calls).toEqual([["locate_download", { id: 1 }]]);
  });

  it("shows nothing when there is nothing else to offer", async () => {
    invoke.mockResolvedValue({ relinked: true, offer: null });
    expect((await locateOne(gone(1))).offer).toBeNull();
  });

  it("shows nothing when the picker was cancelled", async () => {
    invoke.mockResolvedValue({ relinked: false, offer: null });
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
    invoke.mockResolvedValue({ relinked: 2, not_found: 1 });
    const rows = [gone(1), gone(2), snap({ id: 3, status: "completed" }), gone(4)];
    expect(await locateFolderFor(rows)).toBe(
      "Relinked 2 downloads. 1 wasn't found in that folder.",
    );
    expect(invoke.mock.calls).toEqual([["locate_folder", { ids: [1, 2, 4] }]]);
  });

  it("asks nothing when no row is missing, and stays quiet when cancelled", async () => {
    expect(await locateFolderFor([snap({ status: "completed" })])).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
    invoke.mockResolvedValue(null);
    expect(await locateFolderFor([gone(1), gone(2)])).toBeNull();
  });
});

describe("copy", () => {
  it("words the offer and the folder result for one and many", () => {
    expect(sameFolderOfferLine(1)).toMatch(/^1 other missing download from/);
    expect(sameFolderOfferLine(3)).toMatch(/^3 other missing downloads from/);
    expect(locateFolderMessage(0, 3)).toBe("None of the 3 downloads' files are in that folder.");
    expect(locateFolderMessage(3, 0)).toBe("Relinked 3 downloads.");
  });
});
