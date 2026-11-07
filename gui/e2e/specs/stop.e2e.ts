// End-to-end: Stop asks first. Stop opens a dialog (Cancel leaves the download
// running; Tab stays inside it; it closes by itself if the download finishes
// first, and focus returns to the row each way); Keep leaves the row Stopped
// with its partial file and a Resume; Delete leaves it Stopped with
// "Download again" and nothing on disk; a kept
// partial can be deleted from the row, and survives Remove from list. The row
// is tracked by id and its paths re-read from the engine after every restart,
// since a restart from zero may move it to `name (1)`.

import { existsSync, rmSync } from "node:fs";
import { $, browser, expect } from "@wdio/globals";
import {
  Key,
  STOP_DIALOG,
  TOAST,
  addThroughDialog,
  attr,
  baseUrl,
  clickRowAction,
  dismissOverlays,
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

/** Whether the focused element matches `selector`. */
const activeIs = (selector: string) =>
  browser.execute((sel) => document.activeElement?.matches(sel) ?? false, selector);

/** The `.rdm` sidecar beside a `.part` file. */
const sidecarOf = (part: string) => `${part.slice(0, -".part".length)}.rdm`;

/** Waits until neither the partial file nor its sidecar is on disk. */
async function waitForNoPartial(part: string): Promise<void> {
  await browser.waitUntil(() => !existsSync(part) && !existsSync(sidecarOf(part)), {
    timeoutMsg: `${part} (or its sidecar) is still on disk`,
  });
}

/** Restarts a stopped row with its labelled action and waits for bytes again. */
async function restart(id: number, label: "Resume" | "Download again"): Promise<string> {
  await expect(rowById(id).$(`button[aria-label="${label}"]`)).toHaveText(label, {
    containing: true,
  });
  await clickRowAction(id, label);
  await waitForStatus(id, "downloading");
  return waitForPartBytes(id);
}

describe("rdm GUI stop dialog", () => {
  let id = 0;
  let part = "";
  const extra: number[] = [];

  before(async () => {
    await selectFilter("all");
    id = await addThroughDialog(`${baseUrl}/crawl`);
    await waitForStatus(id, "downloading");
    part = await waitForPartBytes(id);
  });

  after(async () => {
    await dismissOverlays();
    if (await $(STOP_DIALOG).isExisting()) {
      await $(STOP_DIALOG).$("button=Cancel").click();
    }
    if (id) await removeAll([id, ...extra]);
    // The download is gone either way, so whatever partial is left is ours.
    if (part) {
      rmSync(part, { force: true });
      rmSync(sidecarOf(part), { force: true });
    }
  });

  it("asks before stopping, and Cancel leaves the download running", async () => {
    await clickRowAction(id, "Stop");
    const dialog = $(STOP_DIALOG);
    await dialog.waitForDisplayed();
    await expect(dialog).toHaveText("Stop this download?", { containing: true });
    await expect(dialog.$('input[name="stop-mode"][value="keep"]')).toBeSelected();
    await dialog.$("button=Cancel").click();
    await waitForGone(STOP_DIALOG);
    await expect(rowById(id)).toHaveAttribute("data-status", "downloading");
  });

  it("keeps Tab inside the dialog, and Escape hands focus back to the row's Stop", async () => {
    await clickRowAction(id, "Stop");
    const dialog = $(STOP_DIALOG);
    await dialog.waitForDisplayed();
    await browser.waitUntil(() => activeIs('input[name="stop-mode"][value="keep"]'), {
      timeoutMsg: "the Stop dialog didn't focus its Keep choice",
    });
    // Keep choice, Cancel, Stop, then round to the Keep choice again; and back.
    for (let i = 0; i < 3; i++) await browser.keys(Key.Tab);
    expect(await activeIs('input[name="stop-mode"][value="keep"]')).toBe(true);
    await browser.keys([Key.Shift, Key.Tab]);
    expect(await activeIs(`${STOP_DIALOG} button.btn-primary`)).toBe(true);

    await browser.keys(Key.Escape);
    await waitForGone(STOP_DIALOG);
    await browser.waitUntil(() => activeIs(`${rowSelector(id)} button[aria-label="Stop"]`), {
      timeoutMsg: `focus didn't return to download ${id}'s Stop button`,
    });
    await expect(rowById(id)).toHaveAttribute("data-status", "downloading");
  });

  it("closes the dialog by itself when the download finishes under it", async () => {
    const quick = await addThroughDialog(`${baseUrl}/slow`);
    extra.push(quick);
    await waitForStatus(quick, "downloading");
    await clickRowAction(quick, "Stop");
    await $(STOP_DIALOG).waitForDisplayed();
    await waitForStatus(quick, "completed", 30_000);
    await waitForGone(STOP_DIALOG, "the Stop dialog stayed open over a finished download");
    await browser.waitUntil(() => activeIs(`${rowSelector(quick)} .c-act button`), {
      timeoutMsg: `focus didn't return to download ${quick}'s row`,
    });
  });

  it("keeps the partial file on Keep, and Resume continues it", async () => {
    await stopThroughDialog(id, "keep");
    await waitForStatus(id, "cancelled");
    // Once the stop lands, focus moves to the stopped row's first action.
    await browser.waitUntil(() => activeIs(`${rowSelector(id)} button[aria-label="Resume"]`), {
      timeoutMsg: `focus didn't move to download ${id}'s Resume after the stop`,
    });
    await waitForAttr(id, "data-partial-kept", "true");
    expect(existsSync(part)).toBe(true);
    expect(existsSync(sidecarOf(part))).toBe(true);

    part = await restart(id, "Resume");
  });

  it("deletes the partial file on Delete, offering Download again", async () => {
    await stopThroughDialog(id, "delete");
    await waitForStatus(id, "cancelled");
    await waitForAttr(id, "data-partial-kept", "false");
    await waitForNoPartial(part);
    await expect(rowById(id).$('button[aria-label="Download again"]')).toBeExisting();
  });

  it("deletes a kept partial file from the row", async () => {
    part = await restart(id, "Download again");
    await stopThroughDialog(id, "keep");
    await waitForAttr(id, "data-partial-kept", "true");
    expect(existsSync(part)).toBe(true);

    await clickRowAction(id, "Delete partial file");
    await waitForAttr(id, "data-partial-kept", "false");
    await expect(rowById(id)).toHaveAttribute("data-status", "cancelled");
    await waitForNoPartial(part);
    await expect(rowById(id).$('button[aria-label="Download again"]')).toBeExisting();
  });

  it("keeps a kept partial file on disk when the row is removed from the list", async () => {
    part = await restart(id, "Download again");
    const filename = await attr(id, "data-filename");
    await stopThroughDialog(id, "keep");
    await waitForAttr(id, "data-partial-kept", "true");

    await clickRowAction(id, "Remove from list");
    await expect($(TOAST)).toHaveText("Its partial file stays on disk", { containing: true });
    await expect($(TOAST)).toHaveText(filename!, { containing: true });
    await waitForGone(rowSelector(id));
    id = 0;
    expect(existsSync(part)).toBe(true);
  });
});
