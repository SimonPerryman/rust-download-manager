// End-to-end: the lanes and the details pane. A 4-connection download held by
// the fixture's gate (/seg-gated: no piece can finish, so none is stolen)
// shows one hairline per piece in its row and, in the details pane, one row
// per live lane (connection) as the ramp opens them, up to 4; pausing merges
// them into one row and resuming brings them back. Lanes are counted in the
// DOM, never inferred from progress bytes. A finished download's pane drops
// the lane rows for one finish bar and a summary line, sums it up and
// verifies a SHA-256 pasted into it.

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { $, $$, browser, expect } from "@wdio/globals";
import {
  addThroughDialog,
  baseUrl,
  clickRowAction,
  dismissOverlays,
  engineRow,
  rectOf,
  removeAll,
  retype,
  rowById,
  rowSelector,
  selectFilter,
  waitForGone,
  waitForStatus,
  waitForText,
} from "../helpers";

const PANE = '[data-testid="details-pane"]';
const paneSelector = (id: number) => `${PANE}[data-id="${id}"]`;
const CHECKSUM = `${PANE} [data-testid="checksum"]`;
const HASH_INPUT = `${CHECKSUM} input[aria-label="SHA-256 hash to compare"]`;

const CELL_LANE = '[data-testid="lanes"][data-size="cell"] [data-testid="lane"]';
const rowsSelector = (pane: string, state: string) =>
  `${pane} [data-testid="lane-rows"][data-state="${state}"]`;
const headSelector = (pane: string) => `${pane} [data-testid="lanes-head"]`;

/** How many of `selector` exist (0 while a re-render has none). */
async function count(selector: string): Promise<number> {
  try {
    return await $$(selector).length;
  } catch {
    return 0;
  }
}

/** The hairlines the row's Progress cell draws, one per piece. */
async function waitForCellLanes(id: number, n: number): Promise<void> {
  await browser.waitUntil(async () => (await count(`${rowSelector(id)} ${CELL_LANE}`)) === n, {
    timeout: 20_000,
    interval: 200,
    timeoutMsg: `download ${id}'s row never showed ${n} lanes`,
  });
}

/** The lane numbers the pane's rows show, top to bottom, while they are lane rows. */
async function shownLanes(pane: string): Promise<string[]> {
  return browser.execute(
    (sel) =>
      [...document.querySelectorAll(`${sel} [data-testid="lane-row"]:not(.leaving)`)].map(
        (r) => r.getAttribute("data-lane") ?? "",
      ),
    rowsSelector(pane, "lanes"),
  );
}

/** Waits for the pane to show lane rows 1..n, each on a piece of its own. */
async function waitForLaneRows(pane: string, n: number): Promise<void> {
  const want = Array.from({ length: n }, (_, i) => String(i + 1));
  await browser.waitUntil(
    async () => JSON.stringify(await shownLanes(pane)) === JSON.stringify(want),
    { timeout: 30_000, interval: 200, timeoutMsg: `the pane never showed lanes ${want}` },
  );
  expect(await count(`${rowsSelector(pane, "lanes")} [data-testid="lane-now"]`)).toBe(n);
}

/** Opens the details pane for `id` by clicking its row (outside the buttons). */
async function openDetails(id: number): Promise<void> {
  await dismissOverlays();
  await rowById(id).$("td.c-name").click();
  await $(paneSelector(id)).waitForDisplayed({ timeoutMsg: `no details pane for ${id}` });
  await expect(rowById(id)).toHaveAttribute("data-selected", "true");
}

async function closeDetails(): Promise<void> {
  if (!(await $(PANE).isExisting())) return;
  await $(`${PANE} button[aria-label="Close details"]`).click();
  await waitForGone(PANE, "the details pane never closed");
}

async function waitForChecksumState(state: string): Promise<void> {
  await browser.waitUntil(async () => (await $(CHECKSUM).getAttribute("data-state")) === state, {
    timeoutMsg: `the checksum never reached ${state}`,
  });
}

/** Opens the Verify form, types `text` into it (element send-keys) and submits. */
async function verify(text: string): Promise<void> {
  if (!(await $(HASH_INPUT).isExisting())) await $(CHECKSUM).$("button=Verify checksum").click();
  await retype($(HASH_INPUT), text, { enter: false });
  await $(CHECKSUM).$("button=Verify").click();
}

describe("rdm GUI lanes and details pane", () => {
  const ids: number[] = [];

  after(async () => {
    await closeDetails();
    await removeAll(ids);
  });

  it("draws a row per live lane in the details pane, and merges them on pause", async () => {
    await selectFilter("all");
    const id = await addThroughDialog(`${baseUrl}/seg-gated`);
    ids.push(id);
    await waitForStatus(id, "downloading", 30_000);

    await waitForCellLanes(id, 4);
    // Drawn, not just in the DOM: the size classes set the lanes' line height.
    expect((await rectOf(`${rowSelector(id)} ${CELL_LANE}`)).height).toBeGreaterThan(0);

    await openDetails(id);
    const pane = paneSelector(id);
    await expect($(`${pane} button[role="tab"][data-tab="overview"]`)).toHaveAttribute(
      "aria-selected",
      "true",
    );
    // The ramp opens lanes one by one; the gate holds every piece, so the
    // tiling stays the 4-way plan and each lane keeps its own piece.
    await waitForLaneRows(pane, 4);
    expect(
      (await rectOf(`${rowsSelector(pane, "lanes")} [data-testid="lane-row"]`)).height,
    ).toBeGreaterThan(0);
    await waitForText(headSelector(pane), "4 lanes · 4 of 4 pieces left");
    expect(await count(`${rowSelector(id)} ${CELL_LANE}`)).toBe(4);

    // Paused, no lane is live: one row holds the work so far.
    await clickRowAction(id, "Pause");
    await waitForStatus(id, "paused");
    await $(rowsSelector(pane, "single")).waitForExist({ timeoutMsg: "pause kept the lane rows" });
    await waitForText(headSelector(pane), "Paused · 4 of 4 pieces left");
    await waitForGone(`${pane} [data-testid="lane-now"]`, "a paused lane still showed a piece");

    await clickRowAction(id, "Resume");
    await waitForStatus(id, "downloading", 30_000);
    await $(rowsSelector(pane, "lanes")).waitForExist({
      timeout: 20_000,
      timeoutMsg: "resume never brought the lane rows back",
    });

    await $(`${pane} button[role="tab"][data-tab="speed"]`).click();
    await $(`${pane} [data-testid="speed-graph"]`).waitForExist();
    await $(`${pane} button[role="tab"][data-tab="overview"]`).click();
    await $(rowsSelector(pane, "lanes")).waitForExist({
      timeoutMsg: "the lane rows didn't return",
    });

    await closeDetails();
    await expect(rowById(id)).not.toHaveAttribute("data-selected");
    // Closing from inside the pane hands focus back to the row's details button.
    await browser.waitUntil(
      () =>
        browser.execute(
          (sel) => document.activeElement === document.querySelector(sel),
          `${rowSelector(id)} .fn-btn`,
        ),
      { timeoutMsg: `focus didn't return to download ${id}'s row` },
    );

    // A click in the actions cell, beside its buttons, isn't a row selection.
    await browser.execute(
      (sel) => (document.querySelector(sel) as HTMLElement).click(),
      `${rowSelector(id)} td.c-act`,
    );
    await browser.pause(300);
    await expect($(PANE)).not.toExist();

    // The filename button is a disclosure: a second press collapses the pane.
    const fnBtn = rowById(id).$(".fn-btn");
    await fnBtn.click();
    await expect(fnBtn).toHaveAttribute("aria-expanded", "true");
    await fnBtn.click();
    await waitForGone(PANE, "the filename button never collapsed the pane");
    await expect(fnBtn).toHaveAttribute("aria-expanded", "false");
  });

  it("swaps a finished download's lane rows for one finish bar and a summary", async () => {
    const id = await addThroughDialog(`${baseUrl}/seg`);
    ids.push(id);
    await waitForStatus(id, "completed", 60_000);
    await openDetails(id);
    const pane = paneSelector(id);
    const done = rowsSelector(pane, "done");
    await $(done).waitForExist({ timeoutMsg: "the finished download kept its lane rows" });
    expect(await count(`${done} [data-testid="lane-row"]`)).toBe(1);
    expect(await count(`${pane} [data-testid="lane-now"]`)).toBe(0);
    await waitForText(headSelector(pane), "Fetched in ");
    const head = await browser.execute(
      (sel) => (document.querySelector(sel) as HTMLElement).innerText,
      headSelector(pane),
    );
    expect(head).toMatch(/^Fetched in \d+ pieces · (up to \d lanes|on 1 lane)$/);
    await closeDetails();
  });

  it("sums up a finished download and verifies a pasted SHA-256", async () => {
    const id = await addThroughDialog(`${baseUrl}/file`);
    ids.push(id);
    await waitForStatus(id, "completed", 30_000);
    const d = await engineRow(id);
    const file = d?.final_path ?? d?.target;
    if (!file) throw new Error(`download ${id} has no file`);
    const right = createHash("sha256").update(readFileSync(file)).digest("hex");
    const wrong = right.startsWith("f") ? `e${right.slice(1)}` : `f${right.slice(1)}`;

    await openDetails(id);
    const summary = $(`${paneSelector(id)} [data-testid="summary"]`);
    await expect(summary).toHaveText(expect.stringContaining("no checksum given"));

    await verify("not a hash");
    await expect($(`${CHECKSUM} [data-testid="verify-error"]`)).toHaveText(
      expect.stringContaining("isn't a SHA-256 hash"),
    );

    await verify(wrong);
    await waitForChecksumState("mismatch");
    // The form closes on a result; focus lands back on its opener, not <body>.
    await browser.waitUntil(
      () =>
        browser.execute(
          (sel) => document.activeElement?.matches(sel) ?? false,
          `${CHECKSUM} button.btn-link`,
        ),
      { timeoutMsg: "focus didn't return to Verify checksum" },
    );
    await expect(summary).toHaveText(expect.stringContaining("checksum doesn't match"));

    await verify(right);
    await waitForChecksumState("verified");
    await expect(summary).toHaveText(expect.stringContaining("checksum verified"));
  });
});
