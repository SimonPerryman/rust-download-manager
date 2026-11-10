// End-to-end: the archive. Archive finished moves Completed and Stopped rows out of
// the working views into Archived at once, behind an undo toast whose Undo
// brings them back; an archived row shows only under Archived, and Unarchive
// returns it. Remove, offered only in Archived, drops the record and leaves
// the file on disk, and asks first: Cancel keeps the row, "Don't ask again"
// skips the next ask, and the Settings toggle turns asking back on.

import { existsSync } from "node:fs";
import { $, $$, browser, expect } from "@wdio/globals";
import {
  TOAST,
  UNDO_TOAST,
  addThroughDialog,
  baseUrl,
  clickOverlay,
  clickRow,
  clickRowAction,
  dismissOverlays,
  engineRow,
  engineRows,
  filterCount,
  invoke,
  pressOnTable,
  removeAll,
  rowById,
  rowSelector,
  selectFilter,
  settingsButton,
  stopThroughDialog,
  waitForGone,
  waitForStatus,
  waitForText,
} from "../helpers";

/** `prefs.ts`'s CONFIRM_REMOVE_KEY. */
const CONFIRM_REMOVE_KEY = "rdm.gui.confirmRemove";
const REMOVE_DIALOG = '[data-testid="remove-dialog"]';
const ASK_TOGGLE = 'input[aria-label="Ask before removing"]';

const toolbarButton = (label: string) => $(`header.toolbar button[aria-label="${label}"]`);

/** Waits until the Archived count reads `n`. */
async function waitForArchivedCount(n: number, timeoutMsg: string): Promise<void> {
  await browser.waitUntil(async () => (await filterCount("archived")) === n, { timeoutMsg });
}

/** Adds `/file` and waits for it to finish; returns its id. */
async function addDone(): Promise<number> {
  const id = await addThroughDialog(`${baseUrl}/file`);
  await waitForStatus(id, "completed", 30_000);
  return id;
}

const removeDialog = () => $(REMOVE_DIALOG);

/** Opens the Remove dialog for the selected row from the Archived view's toolbar. */
async function removeFromToolbar(): Promise<void> {
  await toolbarButton("Remove").click();
  await removeDialog().waitForDisplayed();
}

describe("rdm GUI archive", () => {
  const ours: number[] = [];

  before(async () => {
    await browser.execute((key) => localStorage.removeItem(key), CONFIRM_REMOVE_KEY);
    await selectFilter("all");
  });

  after(async () => {
    await browser.execute((key) => localStorage.removeItem(key), CONFIRM_REMOVE_KEY);
    await removeAll(ours);
  });

  it("archives done and stopped rows with Archive finished, and Undo brings them back", async () => {
    const done = await addDone();
    ours.push(done);
    const stopped = await addThroughDialog(`${baseUrl}/crawl`);
    ours.push(stopped);
    await waitForStatus(stopped, "downloading");
    await stopThroughDialog(stopped, "delete");
    await waitForStatus(stopped, "cancelled");

    await dismissOverlays();
    const archivedBefore = await filterCount("archived");
    const taken = (await engineRows()).filter(
      (d) => !d.archived && (d.status === "completed" || d.status === "cancelled"),
    );
    expect(taken.map((d) => d.id)).toEqual(expect.arrayContaining([done, stopped]));

    await toolbarButton("Archive finished").click();
    for (const id of [done, stopped]) {
      await waitForGone(rowSelector(id), `download ${id} stayed in All`);
    }
    await waitForArchivedCount(
      archivedBefore + taken.length,
      "the Archived count didn't take the archived rows",
    );
    // clickOverlay treats a vanished overlay as done; here that would hide a missed window.
    if (!(await $(UNDO_TOAST).isExisting())) {
      throw new Error("the undo window ended before Undo could be clicked");
    }
    await clickOverlay(UNDO_TOAST, "button=Undo");

    for (const id of [done, stopped]) {
      await rowById(id).waitForExist({ timeoutMsg: `download ${id} didn't come back on Undo` });
    }
    await waitForArchivedCount(archivedBefore, "the Archived count didn't drop back on Undo");
    await expect($(TOAST)).toHaveText(/Restored/);
    for (const id of [done, stopped]) expect((await engineRow(id))?.archived).toBe(false);
  });

  it("shows an archived row only under Archived, and Unarchive returns it", async () => {
    const id = ours[0];
    await clickRowAction(id, "Archive");
    await waitForGone(rowSelector(id), `download ${id} stayed in All once archived`);
    await selectFilter("done");
    expect(await $$(rowSelector(id)).length).toBe(0);

    await selectFilter("archived");
    await waitForText("h1", "Archived");
    await rowById(id).waitForExist({ timeoutMsg: `download ${id} isn't under Archived` });
    await clickRow(id);
    await toolbarButton("Unarchive").click();
    await waitForGone(rowSelector(id), `download ${id} stayed under Archived`);

    await selectFilter("all");
    await rowById(id).waitForExist({ timeoutMsg: `download ${id} didn't return to All` });
    await selectFilter("done");
    await rowById(id).waitForExist({ timeoutMsg: `download ${id} didn't return to Completed` });
    await selectFilter("all");
  });

  it("removes an archived download for good, leaving its file on disk", async () => {
    const id = ours[0];
    const d = (await engineRow(id))!;
    const file = d.final_path ?? d.target;
    expect(existsSync(file)).toBe(true);

    await clickRowAction(id, "Archive");
    await waitForGone(rowSelector(id));
    await selectFilter("archived");
    await clickRow(id);
    await removeFromToolbar();
    await removeDialog().$("button=Remove").click();
    await waitForGone(REMOVE_DIALOG);
    await waitForGone(rowSelector(id), `download ${id} was never removed`);
    expect(await engineRow(id)).toBeUndefined();
    expect(existsSync(file)).toBe(true);
    await selectFilter("all");
  });

  it("asks before removing, until told not to, and Settings turns asking back on", async () => {
    const [kept, ticked, skipped, again] = [
      await addDone(),
      await addDone(),
      await addDone(),
      await addDone(),
    ];
    ours.push(kept, ticked, skipped, again);
    await invoke("archive", { ids: [kept, ticked, skipped, again] });
    await selectFilter("archived");
    for (const id of [kept, ticked, skipped, again]) {
      await rowById(id).waitForExist({ timeoutMsg: `download ${id} isn't under Archived` });
    }

    // Cancel keeps the row; confirming drops it.
    await clickRow(kept);
    await removeFromToolbar();
    await removeDialog().$("button=Cancel").click();
    await waitForGone(REMOVE_DIALOG);
    await expect(rowById(kept)).toBeExisting();
    expect(await engineRow(kept)).toBeDefined();
    await removeFromToolbar();
    await removeDialog().$("button=Remove").click();
    await waitForGone(rowSelector(kept), `download ${kept} was never removed`);

    // Del asks too; "Don't ask again" turns asking off.
    await clickRow(ticked);
    await pressOnTable("Delete");
    await removeDialog().waitForDisplayed();
    await removeDialog().$('input[aria-label="Don\'t ask again"]').click();
    await removeDialog().$("button=Remove").click();
    await waitForGone(rowSelector(ticked), `download ${ticked} was never removed`);
    expect(await browser.execute((key) => localStorage.getItem(key), CONFIRM_REMOVE_KEY)).toBe(
      "false",
    );

    // Now Del removes at once: an open dialog would hold the row back.
    await clickRow(skipped);
    await pressOnTable("Delete");
    await waitForGone(rowSelector(skipped), `download ${skipped} was never removed`);
    expect(await $$(REMOVE_DIALOG).length).toBe(0);

    // Settings turns asking back on.
    await settingsButton().click();
    const toggle = $(ASK_TOGGLE);
    await expect(toggle).not.toBeSelected();
    await toggle.click();
    await expect(toggle).toBeSelected();
    await settingsButton().click();
    await waitForGone(ASK_TOGGLE);

    await clickRow(again);
    await pressOnTable("Delete");
    await removeDialog().waitForDisplayed();
    await removeDialog().$("button=Cancel").click();
    await waitForGone(REMOVE_DIALOG);
    await expect(rowById(again)).toBeExisting();
    await selectFilter("all");
  });
});
