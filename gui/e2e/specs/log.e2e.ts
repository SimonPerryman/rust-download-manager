// End-to-end: the details pane's Log tab. A download that fails against
// /seg-403 shows its engine lines, the error among them, in its Log tab; the
// tab follows the details pane to another download (a finished one ends on
// its Done summary), the Detailed toggle adds the per-request lines and
// takes them away again, it says so when a download hasn't logged anything
// (one waiting in the queue), keeps polling while it's open (a running
// download's pause shows up without reopening it), and the list goes away
// when another tab is chosen or the pane closes.

import { $, $$, browser, expect } from "@wdio/globals";
import {
  addThroughDialog,
  baseUrl,
  dismissOverlays,
  invoke,
  removeAll,
  rowById,
  selectFilter,
  waitForGone,
  waitForStatus,
  waitForText,
} from "../helpers";

const PANE = '[data-testid="details-pane"]';
const paneSelector = (id: number) => `${PANE}[data-id="${id}"]`;
const LOG = `${PANE} [data-testid="log"]`;
const DETAILED = `${PANE} [data-testid="log-detailed"]`;
/** Where the Log tab keeps the Detailed choice (see gui/src/lib/prefs.ts). */
const LOG_DETAILED_KEY = "rdm.gui.logDetailed";
const tab = (key: string) => $(`${PANE} button[role="tab"][data-tab="${key}"]`);

/** Opens the details pane for `id` by clicking its row (outside the buttons). */
async function openDetails(id: number): Promise<void> {
  await dismissOverlays();
  await rowById(id).$("td.c-name").click();
  await $(paneSelector(id)).waitForDisplayed({ timeoutMsg: `no details pane for ${id}` });
}

describe("rdm GUI log tab", () => {
  const ids: number[] = [];
  let failed = 0;
  let done = 0;
  let running = 0;

  before(async () => {
    // A run that died with Detailed on would otherwise start with it on.
    await browser.execute((key) => localStorage.removeItem(key), LOG_DETAILED_KEY);
    await selectFilter("all");
    failed = await addThroughDialog(`${baseUrl}/seg-403`);
    ids.push(failed);
    done = await addThroughDialog(`${baseUrl}/file`);
    ids.push(done);
    // Held open by the fixture's gate, so it's still running when its Log opens.
    running = await addThroughDialog(`${baseUrl}/gated`);
    ids.push(running);
    await waitForStatus(failed, "failed", 30_000);
    await waitForStatus(done, "completed", 30_000);
  });

  after(async () => {
    try {
      if (await $(PANE).isExisting()) {
        await $(`${PANE} button[aria-label="Close details"]`).click();
        await waitForGone(PANE);
      }
      await browser.execute((key) => localStorage.removeItem(key), LOG_DETAILED_KEY);
    } finally {
      await removeAll(ids);
    }
  });

  it("shows a failed download's error lines in its Log tab", async () => {
    await openDetails(failed);
    await tab("log").click();
    await expect(tab("log")).toHaveAttribute("aria-selected", "true");
    await waitForText(LOG, "Started · 4.00 MB · resumable");
    await waitForText(LOG, "Failed");
    await waitForText(LOG, "server returned HTTP 403");
    await expect($(`${LOG} li[data-level="error"]`)).toBeExisting();
  });

  it("follows the details pane to another download", async () => {
    await openDetails(done);
    await expect(tab("log")).toHaveAttribute("aria-selected", "true");
    await waitForText(LOG, "Saved to ");
    await waitForText(LOG, "Done · 97.6 KB in ");
    await waitForGone(`${LOG} li[data-level="error"]`, "the other download's error stayed");
  });

  it("adds the per-request lines while Detailed is on", async () => {
    expect(await $(DETAILED).isSelected()).toBe(false);
    expect(await $$(`${LOG} li[data-level="debug"]`).length).toBe(0);
    await $(DETAILED).click();
    try {
      await waitForText(LOG, "Requested the file · HTTP 200");
      expect(await $$(`${LOG} li[data-level="debug"]`).length).toBeGreaterThan(0);
    } finally {
      // Remembered across downloads (and launches), so leave it off.
      if (await $(DETAILED).isSelected()) await $(DETAILED).click();
    }
    await waitForGone(`${LOG} li[data-level="debug"]`, "the detail stayed after Detailed went off");
    await waitForText(LOG, "Done · ");
  });

  it("drops the list when another tab is chosen", async () => {
    await tab("overview").click();
    await waitForGone(LOG, "the log stayed after leaving its tab");
    await tab("log").click();
    await waitForText(LOG, "Done · ");
  });

  it("says nothing is logged yet for a download that hasn't run", async () => {
    await waitForStatus(running, "downloading", 30_000);
    // A cap of 1, held by the running download, keeps the next one waiting.
    await invoke("set_settings", { maxConcurrent: 1, globalLimit: null, maxConnections: null });
    try {
      const waiting = await addThroughDialog(`${baseUrl}/gated`);
      ids.push(waiting);
      await waitForStatus(waiting, "queued");
      await openDetails(waiting);
      await expect(tab("log")).toHaveAttribute("aria-selected", "true");
      await waitForText(`${PANE} [data-testid="log-empty"]`, "Nothing logged yet");
      expect(await $(LOG).isExisting()).toBe(false);
    } finally {
      await invoke("set_settings", { maxConcurrent: 3, globalLimit: null, maxConnections: null });
    }
  });

  it("keeps polling while open: a running download's new lines appear", async () => {
    await waitForStatus(running, "downloading", 30_000);
    await openDetails(running);
    await expect(tab("log")).toHaveAttribute("aria-selected", "true");
    await waitForText(LOG, "Started · size unknown · not resumable · 1 lane");
    await invoke("pause", { id: running });
    await waitForText(LOG, "Paused");
  });

  it("drops the list when the pane closes", async () => {
    await $(`${PANE} button[aria-label="Close details"]`).click();
    await waitForGone(LOG, "the log stayed after the pane closed");
    await waitForGone(PANE, "the details pane never closed");
  });
});
