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

/** The rendered text of the first match of `selector`. */
async function innerText(selector: string): Promise<string> {
  return browser.execute(
    (sel) => (document.querySelector(sel) as HTMLElement | null)?.innerText ?? "",
    selector,
  );
}

/** The accessible names of the pane's live lane rows, top to bottom. */
async function rowNames(pane: string): Promise<string[]> {
  return browser.execute(
    (sel) =>
      [...document.querySelectorAll(`${sel} [data-testid="lane-row"]:not(.leaving)`)].map(
        (r) => r.getAttribute("aria-label") ?? "",
      ),
    rowsSelector(pane, "lanes"),
  );
}

/**
 * Points at lane `lane`'s row (a dispatched pointermove at its centre, which
 * every driver delivers the same way) and checks the tooltip names it.
 */
async function expectTooltipNamesLane(pane: string, lane: number): Promise<void> {
  const row = `${rowsSelector(pane, "lanes")} [data-testid="lane-row"][data-lane="${lane}"]`;
  const at = await rectOf(row);
  await browser.execute(
    (sel, x, y) => {
      const el = document.querySelector(sel)!;
      const init = { bubbles: true, clientX: x, clientY: y, pointerType: "mouse" };
      el.dispatchEvent(new PointerEvent("pointerover", init));
      el.dispatchEvent(new PointerEvent("pointermove", init));
    },
    row,
    at.x + at.width / 2,
    at.y + at.height / 2,
  );
  await waitForText('[data-testid="lane-tip"]', `Lane ${lane} · piece `);
  expect(await innerText('[data-testid="lane-tip"]')).toMatch(
    new RegExp(`^Lane ${lane} · piece \\d+ · \\S+–\\S+ \\S+ · \\d+% downloaded$`),
  );
  // React's pointer-leave comes from a pointerout to somewhere outside.
  await browser.execute((sel) => {
    document
      .querySelector(sel)!
      .dispatchEvent(
        new PointerEvent("pointerout", { bubbles: true, relatedTarget: document.body }),
      );
  }, row);
  await waitForGone('[data-testid="lane-tip"]', "the lane tooltip stayed after the pointer left");
}

/**
 * Up to 16 lanes can be open, so the rows block stays four rows tall and
 * scrolls in place: with 12 more rows (copies of a live one, removed after)
 * neither the block nor the scale under it moves, and the scale still lines
 * up with the tracks.
 */
async function expectBoundedRows(pane: string): Promise<void> {
  const rows = rowsSelector(pane, "lanes");
  const scale = `${pane} .lane-scale`;
  // Measured once the rows have finished sliding in.
  await browser.waitUntil(
    () =>
      browser.execute(
        (sel) => document.querySelector(sel)?.getAnimations({ subtree: true }).length === 0,
        rows,
      ),
    { timeoutMsg: "the lane rows kept animating" },
  );
  const block = await rectOf(rows);
  const scaleAt = await rectOf(scale);
  await browser.execute((sel) => {
    const box = document.querySelector(sel)!;
    const row = box.querySelector('[data-testid="lane-row"]')!;
    for (let i = 0; i < 12; i++) {
      const copy = row.cloneNode(true) as HTMLElement;
      copy.dataset.testid = "lane-row-copy";
      // Full height at once: a live row slides in from nothing.
      copy.style.animation = "none";
      box.append(copy);
    }
  }, rows);
  try {
    const grown = await rectOf(rows);
    expect(grown.height).toBeCloseTo(block.height, 0);
    expect((await rectOf(scale)).y).toBeCloseTo(scaleAt.y, 0);
    const scrolls = await browser.execute((sel) => {
      const box = document.querySelector(sel) as HTMLElement;
      return box.scrollHeight > box.clientHeight;
    }, rows);
    expect(scrolls).toBe(true);
    // Scrolling or not, the scale's ends sit at the tracks' ends.
    const track = await rectOf(`${rows} .lane-track`);
    const first = await rectOf(`${scale} span:first-child`);
    const last = await rectOf(`${scale} span:last-child`);
    expect(Math.abs(first.x - track.x)).toBeLessThan(1.5);
    expect(Math.abs(last.x + last.width - (track.x + track.width))).toBeLessThan(1.5);
  } finally {
    await browser.execute((sel) => {
      document.querySelectorAll(`${sel} [data-testid="lane-row-copy"]`).forEach((r) => r.remove());
    }, rows);
  }
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
    await waitForText(headSelector(pane), "4 lanes");
    expect(await innerText(headSelector(pane))).toBe("4 lanes");
    // No label column: each row is named for screen readers instead.
    expect(await rowNames(pane)).toEqual(["Lane 1", "Lane 2", "Lane 3", "Lane 4"]);
    expect(await $(`${pane} .lane-label`).isExisting()).toBe(false);
    await expectTooltipNamesLane(pane, 3);
    expect(await count(`${rowSelector(id)} ${CELL_LANE}`)).toBe(4);
    await expectBoundedRows(pane);

    // Paused, no lane is live: one row holds the work so far.
    await clickRowAction(id, "Pause");
    await waitForStatus(id, "paused");
    await $(rowsSelector(pane, "single")).waitForExist({ timeoutMsg: "pause kept the lane rows" });
    await waitForText(headSelector(pane), "Paused");
    expect(await innerText(headSelector(pane))).toBe("Paused");
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
    await waitForText(headSelector(pane), "Downloaded in ");
    expect(await innerText(headSelector(pane))).toMatch(
      /^Downloaded in \d+ pieces · (up to \d lanes|on 1 lane)$/,
    );
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
