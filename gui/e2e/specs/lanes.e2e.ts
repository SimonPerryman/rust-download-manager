// End-to-end: the lanes and the details pane. A 4-connection download held by
// the fixture's gate (/seg-gated: no piece can finish, so none is stolen)
// shows one lane per piece, 4 in its row and 4 in the details pane; lanes are
// counted in the DOM, never inferred from progress bytes. A finished download's
// pane sums it up and verifies a SHA-256 pasted into it.

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { $, $$, browser, expect } from "@wdio/globals";
import {
  addThroughDialog,
  baseUrl,
  dismissOverlays,
  engineRow,
  removeAll,
  retype,
  rowById,
  rowSelector,
  selectFilter,
  waitForGone,
  waitForStatus,
} from "../helpers";

const PANE = '[data-testid="details-pane"]';
const paneSelector = (id: number) => `${PANE}[data-id="${id}"]`;
const CHECKSUM = `${PANE} [data-testid="checksum"]`;
const HASH_INPUT = `${CHECKSUM} input[aria-label="SHA-256 hash to compare"]`;

/** How many lanes the first `lanes` element under `scope` draws (0 while it has none). */
async function laneCount(scope: string, size: "cell" | "pane"): Promise<number> {
  try {
    return await $$(`${scope} [data-testid="lanes"][data-size="${size}"] [data-testid="lane"]`)
      .length;
  } catch {
    return 0;
  }
}

async function waitForLanes(scope: string, size: "cell" | "pane", n: number): Promise<void> {
  await browser.waitUntil(async () => (await laneCount(scope, size)) === n, {
    timeout: 20_000,
    interval: 200,
    timeoutMsg: `${scope} never showed ${n} ${size} lanes`,
  });
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

  it("draws one lane per piece in the row and in the details pane", async () => {
    await selectFilter("all");
    const id = await addThroughDialog(`${baseUrl}/seg-gated`);
    ids.push(id);
    await waitForStatus(id, "downloading", 30_000);

    await waitForLanes(rowSelector(id), "cell", 4);

    await openDetails(id);
    const pane = paneSelector(id);
    await expect($(`${pane} button[role="tab"][data-tab="overview"]`)).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await waitForLanes(pane, "pane", 4);
    // The gate holds every piece, so the tiling is still the 4-way plan.
    await expect(rowById(id)).toHaveAttribute("data-status", "downloading");
    expect(await laneCount(rowSelector(id), "cell")).toBe(4);

    await $(`${pane} button[role="tab"][data-tab="speed"]`).click();
    await $(`${pane} [data-testid="speed-graph"]`).waitForExist();
    await $(`${pane} button[role="tab"][data-tab="overview"]`).click();
    await waitForLanes(pane, "pane", 4);

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
