// End-to-end: connection ramping. A multi-segment download starts on one
// connection and adds more while its measured speed keeps rising; the row
// shows the live count while it streams over more than one, and drops it once
// paused. The fixture slows each /seg connection (see wdio.conf.ts) so the
// download outlasts the engine's 1 s ramp windows.

import { $, $$, browser } from "@wdio/globals";
import { waitForGone } from "../helpers";

const baseUrl = process.env.RDM_E2E_BASE_URL!;

const ADD_DIALOG = '[role="dialog"][aria-label="Add download"]';
const rowSelector = (id: string) => `li[data-testid="download-row"][data-id="${id}"]`;
const connectionsSelector = (id: string) => `${rowSelector(id)} [data-testid="row-connections"]`;
const rowById = (id: string) => $(rowSelector(id));

/** Ids of every row in the current tab. */
async function rowIds(): Promise<string[]> {
  return $$('li[data-testid="download-row"]').map(
    async (r) => (await r.getAttribute("data-id")) ?? "",
  );
}

async function waitForStatus(id: string, status: string, timeout?: number): Promise<void> {
  await browser.waitUntil(
    async () => {
      const r = rowById(id);
      return (await r.isExisting()) && (await r.getAttribute("data-status")) === status;
    },
    { timeout, timeoutMsg: `download ${id} never reached status "${status}"` },
  );
}

/** The row's live connection count, or 0 while it shows none. */
async function shownConnections(id: string): Promise<number> {
  // A fresh query each poll; a row redrawn mid-read reads as "none yet".
  try {
    const label = $(connectionsSelector(id));
    if (!(await label.isExisting())) return 0;
    const match = /^(\d+) connections$/.exec((await label.getText()).trim());
    return match ? Number(match[1]) : 0;
  } catch {
    return 0;
  }
}

/**
 * Switches list tab. Scrolls to the top first: with a long list, WebDriver
 * scrolls the tab to the window's top edge, under the sticky header.
 */
async function showTab(name: "Active" | "History"): Promise<void> {
  await browser.execute(() => window.scrollTo(0, 0));
  await $(`button*=${name}`).click();
}

describe("rdm GUI connection ramping", () => {
  let id: string | undefined;

  after(async () => {
    if (!id) return;
    await showTab("Active");
    if (await rowById(id).isExisting()) {
      await rowById(id).$('button[aria-label="Cancel"]').click();
    }
    await showTab("History");
    await rowById(id).waitForExist({ timeoutMsg: `download ${id} never reached History` });
    await rowById(id).$('button[aria-label="Remove"]').click();
    await waitForGone(rowSelector(id), `download ${id} was never removed`);
    await showTab("Active");
  });

  it("adds connections while speed rises and shows the live count", async () => {
    const before = await rowIds();
    await $("button=Add download").click();
    const dialog = $(ADD_DIALOG);
    await dialog.waitForDisplayed();
    await dialog.$('input[aria-label="URL"]').setValue(`${baseUrl}/seg`);
    await dialog.$("button=Add download").click();
    await waitForGone(ADD_DIALOG);
    await browser.waitUntil(
      async () => {
        id = (await rowIds()).find((r) => !before.includes(r));
        return id !== undefined;
      },
      { timeoutMsg: "the /seg download never appeared" },
    );
    const own = id!;

    await waitForStatus(own, "downloading", 30_000);
    // The first ramp window is skipped and each later one is 1 s, so allow
    // plenty of slack for a loaded runner.
    await browser.waitUntil(async () => (await shownConnections(own)) >= 2, {
      timeout: 20_000,
      interval: 200,
      timeoutMsg: `download ${own} never showed 2+ connections`,
    });

    await rowById(own).$('button[aria-label="Pause"]').click();
    await waitForStatus(own, "paused");
    await waitForGone(connectionsSelector(own), `download ${own} kept its connection count`);
  });
});
