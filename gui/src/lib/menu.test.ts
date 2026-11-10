import { describe, expect, it } from "vitest";
import { menuItems, selectionActions, type MenuEntry } from "./menu";
import { snap } from "./testSnapshot";
import type { DownloadSnapshot } from "./types";

/** The menu as labels, "-" for a separator and "(note)" for notes. */
const labels = (entries: MenuEntry[]) =>
  entries.map((e) => (e.kind === "item" ? e.label : e.kind === "note" ? `(${e.text})` : "-"));

const menu = (list: DownloadSnapshot[], queue: number[] = [], failed = 0, archived = false) =>
  labels(menuItems(list, queue, failed, archived));

describe("menuItems", () => {
  it("fits a finished download", () => {
    expect(menu([snap({ status: "completed" })])).toEqual([
      "Open",
      "Show in folder",
      "-",
      "Copy link",
      "Download again",
      "Tags…",
      "-",
      "Archive",
    ]);
  });

  it("fits a running download", () => {
    expect(menu([snap({ status: "downloading" })])).toEqual([
      "Pause",
      "Stop…",
      "-",
      "Copy link",
      "Tags…",
    ]);
  });

  it("offers queue moves for one waiting download by its place", () => {
    const q = [1, 2, 3];
    expect(menu([snap({ id: 1 })], q)).toEqual([
      "Pause",
      "Move down",
      "Stop…",
      "-",
      "Copy link",
      "Tags…",
    ]);
    expect(menu([snap({ id: 2 })], q)).toEqual([
      "Pause",
      "Start next",
      "Move up",
      "Move down",
      "Stop…",
      "-",
      "Copy link",
      "Tags…",
    ]);
    expect(menu([snap({ id: 3 })], q)).toEqual([
      "Pause",
      "Start next",
      "Move up",
      "Stop…",
      "-",
      "Copy link",
      "Tags…",
    ]);
  });

  it("offers to delete a kept partial, and archives", () => {
    expect(menu([snap({ status: "cancelled", partial_kept: true })])).toEqual([
      "Resume",
      "-",
      "Copy link",
      "Tags…",
      "-",
      "Delete partial file",
      "Archive",
    ]);
  });

  it("in the archive, unarchives and says a kept partial survives Remove", () => {
    const d = snap({ status: "cancelled", partial_kept: true, archived: true });
    expect(menu([d], [], 0, true)).toEqual([
      "Resume",
      "-",
      "Copy link",
      "Tags…",
      "-",
      "Delete partial file",
      "Unarchive",
      "Remove",
      "(The partial file stays on disk.)",
    ]);
    const done = snap({ status: "completed", archived: true });
    expect(menu([done], [], 0, true)).toEqual([
      "Open",
      "Show in folder",
      "-",
      "Copy link",
      "Download again",
      "Tags…",
      "-",
      "Unarchive",
      "Remove",
      "(The file stays on disk.)",
    ]);
    const del = menuItems([done], [], 0, true).find((e) => e.kind === "item" && e.cmd === "remove");
    expect(del).toMatchObject({ shortcut: "Del" });
  });

  it("archives only finished rows outside the archive", () => {
    expect(menu([snap({ status: "paused" })])).not.toContain("Archive");
    const archive = menuItems([snap({ status: "failed" })], [], 1).find(
      (e) => e.kind === "item" && e.cmd === "archive",
    );
    expect(archive).toMatchObject({ label: "Archive", shortcut: "Del" });
    expect(menu([snap({ status: "completed" })])).not.toContain("Remove");
  });

  it("calls a stopped download with nothing kept Download again", () => {
    expect(menu([snap({ status: "cancelled" })])).toEqual([
      "Download again",
      "-",
      "Copy link",
      "Tags…",
      "-",
      "Archive",
    ]);
    // Space only resumes; it never restarts a download from zero.
    const again = menuItems([snap({ status: "cancelled" })], [], 0).find(
      (e) => e.kind === "item" && e.cmd === "resume",
    );
    expect(again).toMatchObject({ label: "Download again", shortcut: undefined });
  });

  it("offers Retry all failed only when more than one failed", () => {
    const f = snap({ status: "failed" });
    expect(menu([f], [], 1)).toEqual(["Retry", "-", "Copy link", "Tags…", "-", "Archive"]);
    expect(menu([f], [], 2)).toContain("Retry all failed");
  });

  it("leaves Retry all failed out of the archive, where it would retry other rows", () => {
    const f = snap({ status: "failed", archived: true });
    expect(menu([f], [], 2, true)).not.toContain("Retry all failed");
    expect(menu([f], [], 2, true)).toContain("Retry");
  });

  it("in the archive, says Remove deletes a failed download's partial file", () => {
    const f = snap({ status: "failed", archived: true });
    expect(menu([f], [], 0, true).slice(-2)).toEqual(["Remove", "(Its partial file is deleted.)"]);
    const done = snap({ id: 2, status: "completed", archived: true });
    expect(menu([f, done], [], 0, true).slice(-1)).toEqual([
      "(The file stays on disk. The failed download's partial file is deleted.)",
    ]);
  });

  it("covers a mixed selection with each action that applies to any of it", () => {
    const list = [
      snap({ id: 1, status: "completed" }),
      snap({ id: 2, status: "downloading" }),
      snap({ id: 3, status: "cancelled", partial_kept: true }),
      snap({ id: 4, status: "cancelled", partial_kept: true }),
    ];
    expect(menu(list)).toEqual([
      "Open",
      "Pause",
      "Resume",
      "Stop…",
      "-",
      "Copy links",
      "Download again",
      "Tags…",
      "-",
      "Delete partial files",
      "Archive",
    ]);
  });

  it("leaves Resume out for several stopped rows with nothing kept", () => {
    const list = [snap({ id: 1, status: "cancelled" }), snap({ id: 2, status: "cancelled" })];
    expect(menu(list)).not.toContain("Resume");
    expect(selectionActions(list)).not.toContain("resume");
  });

  it("shows each item's shortcut", () => {
    const entries = menuItems([snap({ id: 2 })], [1, 2], 0);
    const up = entries.find((e) => e.kind === "item" && e.cmd === "moveUp");
    expect(up).toMatchObject({ shortcut: "Alt+↑" });
  });
});

describe("selectionActions", () => {
  it("offers each action that applies to any selected row, in toolbar order", () => {
    expect(selectionActions([snap({ status: "downloading" }), snap({ status: "failed" })])).toEqual(
      ["pause", "stop", "retry", "tags", "archive"],
    );
    expect(selectionActions([snap({ status: "completed" }), snap({ status: "paused" })])).toEqual([
      "open",
      "resume",
      "stop",
      "tags",
      "archive",
    ]);
  });

  it("offers Unarchive and Remove in the archive", () => {
    const list = [
      snap({ id: 1, status: "completed", archived: true }),
      snap({ id: 2, status: "failed", archived: true }),
    ];
    expect(selectionActions(list, true)).toEqual(["open", "retry", "tags", "unarchive", "remove"]);
  });
});
