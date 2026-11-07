// End-to-end: Clear finished. It hides Done and Stopped rows at once behind
// an undo toast; Undo inside the ~5 s window brings them back and nothing
// reaches the engine; without Undo, the engine clears them when the window
// ends, the rows don't flash back, and files and kept partials stay on disk.
// "A row finishing inside the window is spared" is covered by the reducer's
// unit tests (captured ids) and the engine's `clear(statuses, ids)` test: an
// e2e version would race a finish against the window and flake.

import { existsSync, rmSync } from "node:fs";
import { $, $$, browser, expect } from "@wdio/globals";
import {
  TOAST,
  UNDO_TOAST,
  addThroughDialog,
  baseUrl,
  clickOverlay,
  dismissOverlays,
  engineRow,
  engineRows,
  filterCount,
  removeAll,
  rowById,
  rowSelector,
  selectFilter,
  stopThroughDialog,
  waitForAttr,
  waitForGone,
  waitForPartBytes,
  waitForStatus,
} from "../helpers";

/** `pendingClear.ts`'s UNDO_MS: how long the undo window holds the clear back. */
const UNDO_MS = 5_000;

const clearFinished = () => $("button=Clear finished");

/** Which of `ids` the engine still lists. */
async function inEngine(ids: number[]): Promise<number[]> {
  const listed = new Set((await engineRows()).map((d) => d.id));
  return ids.filter((id) => listed.has(id));
}

describe("rdm GUI clear finished", () => {
  let ours: number[] = [];
  let doneFile = "";
  let keptPart = "";

  before(async () => {
    await selectFilter("all");
    const done = await addThroughDialog(`${baseUrl}/file`);
    ours.push(done);
    await waitForStatus(done, "completed", 30_000);
    const d = (await engineRow(done))!;
    doneFile = d.final_path ?? d.target;

    const kept = await addThroughDialog(`${baseUrl}/crawl`);
    ours.push(kept);
    await waitForStatus(kept, "downloading");
    keptPart = await waitForPartBytes(kept);
    await stopThroughDialog(kept, "keep");
    await waitForAttr(kept, "data-partial-kept", "true");

    const deleted = await addThroughDialog(`${baseUrl}/crawl`);
    ours.push(deleted);
    await waitForStatus(deleted, "downloading");
    await stopThroughDialog(deleted, "delete");
    await waitForStatus(deleted, "cancelled");
  });

  after(async () => {
    // Whatever wasn't cleared goes now; a kept partial left behind is ours.
    await removeAll(await inEngine(ours));
    if (keptPart) {
      rmSync(keptPart, { force: true });
      rmSync(`${keptPart.slice(0, -".part".length)}.rdm`, { force: true });
    }
  });

  it("hides done and stopped rows at once, and Undo brings them back untouched", async () => {
    await dismissOverlays();
    await clearFinished().click();
    for (const id of ours) await waitForGone(rowSelector(id), `download ${id} wasn't hidden`);
    // Only the minimum inside the undo window, then straight to Undo.
    expect(await inEngine(ours)).toEqual(ours);
    // clickOverlay treats a vanished overlay as done; here that would hide a missed window.
    if (!(await $(UNDO_TOAST).isExisting())) {
      throw new Error("the undo window ended before Undo could be clicked");
    }
    await clickOverlay(UNDO_TOAST, "button=Undo");

    for (const id of ours) {
      await rowById(id).waitForExist({ timeoutMsg: `download ${id} didn't come back on Undo` });
    }
    await expect($(TOAST)).toHaveText(/Restored \d+ downloads/);

    // Nothing reaches the engine after the window either.
    await browser.pause(UNDO_MS + 1500);
    expect(await inEngine(ours)).toEqual(ours);
    for (const id of ours) await expect(rowById(id)).toBeExisting();
  });

  it("clears them from the engine once the window ends, leaving files on disk", async () => {
    await dismissOverlays();
    const doneBefore = await filterCount("done");
    const stoppedBefore = await filterCount("stopped");

    await clearFinished().click();
    const toast = $(UNDO_TOAST);
    await expect(toast).toHaveText("Cleared", { containing: true });
    await expect(toast).toHaveText("kept partial files stay on disk", { containing: true });
    await browser.waitUntil(
      async () =>
        (await filterCount("done")) <= doneBefore - 1 &&
        (await filterCount("stopped")) <= stoppedBefore - 2,
      { timeoutMsg: "Done and Stopped counts didn't drop by the cleared rows" },
    );

    await browser.waitUntil(async () => (await inEngine(ours)).length === 0, {
      timeout: 15_000,
      timeoutMsg: "the engine never cleared the rows after the undo window",
    });
    // No flash back once the engine's snapshot drops them.
    for (const id of ours) expect(await $$(rowSelector(id)).length).toBe(0);
    await browser.pause(500);
    for (const id of ours) expect(await $$(rowSelector(id)).length).toBe(0);
    ours = [];

    expect(existsSync(doneFile)).toBe(true);
    expect(existsSync(keptPart)).toBe(true);
  });
});
