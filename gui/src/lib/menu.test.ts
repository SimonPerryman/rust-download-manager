import { describe, expect, it } from "vitest";
import { menuItems, selectionActions, type MenuEntry } from "./menu";
import { snap } from "./testSnapshot";
import type { DownloadSnapshot } from "./types";

/** The menu as labels, "-" for a separator and "(note)" for notes. */
const labels = (entries: MenuEntry[]) =>
  entries.map((e) => (e.kind === "item" ? e.label : e.kind === "note" ? `(${e.text})` : "-"));

const menu = (list: DownloadSnapshot[], queue: number[] = [], failed = 0) =>
  labels(menuItems(list, queue, failed));

describe("menuItems", () => {
  it("fits a finished download", () => {
    expect(menu([snap({ status: "completed" })])).toEqual([
      "Open",
      "Show in folder",
      "-",
      "Copy link",
      "Download again",
      "-",
      "Remove from list",
      "(The file stays on disk.)",
    ]);
  });

  it("fits a running download", () => {
    expect(menu([snap({ status: "downloading" })])).toEqual(["Pause", "Stop…", "-", "Copy link"]);
  });

  it("offers queue moves for one waiting download by its place", () => {
    const q = [1, 2, 3];
    expect(menu([snap({ id: 1 })], q)).toEqual(["Pause", "Move down", "Stop…", "-", "Copy link"]);
    expect(menu([snap({ id: 2 })], q)).toEqual([
      "Pause",
      "Start next",
      "Move up",
      "Move down",
      "Stop…",
      "-",
      "Copy link",
    ]);
    expect(menu([snap({ id: 3 })], q)).toEqual([
      "Pause",
      "Start next",
      "Move up",
      "Stop…",
      "-",
      "Copy link",
    ]);
  });

  it("says a kept partial survives Remove and offers to delete it", () => {
    expect(menu([snap({ status: "cancelled", partial_kept: true })])).toEqual([
      "Resume",
      "-",
      "Copy link",
      "-",
      "Delete partial file",
      "Remove from list",
      "(The partial file stays on disk.)",
    ]);
  });

  it("calls a stopped download with nothing kept Download again", () => {
    expect(menu([snap({ status: "cancelled" })])).toEqual([
      "Download again",
      "-",
      "Copy link",
      "-",
      "Remove from list",
    ]);
    // Space only resumes; it never restarts a download from zero.
    const again = menuItems([snap({ status: "cancelled" })], [], 0).find(
      (e) => e.kind === "item" && e.cmd === "resume",
    );
    expect(again).toMatchObject({ label: "Download again", shortcut: undefined });
  });

  it("offers Retry all failed only when more than one failed", () => {
    const f = snap({ status: "failed" });
    expect(menu([f], [], 1)).toEqual(["Retry", "-", "Copy link", "-", "Remove from list"]);
    expect(menu([f], [], 2)).toContain("Retry all failed");
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
      "-",
      "Delete partial files",
      "Remove from list",
      "(Files and kept partial files stay on disk.)",
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
      ["pause", "stop", "retry", "remove"],
    );
    expect(selectionActions([snap({ status: "completed" }), snap({ status: "paused" })])).toEqual([
      "open",
      "resume",
      "stop",
      "remove",
    ]);
  });
});
