// End-to-end: the power features. The right-click menu fits a stopped row
// that kept its partial (Remove says the partial stays; Delete partial file
// deletes it); Ctrl-click selects two rows and the toolbar turns into their
// actions; Shift+Down and Space work the selection from the keyboard; the
// density toggle switches 32/48 px rows and is remembered; a narrow window
// folds the sidebar into an icon rail that still fits the total speed.
// Shares the app state dir with the other specs (see wdio.conf.ts), so it
// removes its downloads and leaves compact rows when done.

import { existsSync } from "node:fs";
import { $, browser, expect } from "@wdio/globals";
import {
  Key,
  ROW_MENU,
  TOAST,
  addThroughDialog,
  baseUrl,
  clickRow,
  invoke,
  openRowMenu,
  pressOnTable,
  rectOf,
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

const SELECTION_BAR = '[data-testid="selection-bar"]';
const PANE = '[data-testid="details-pane"]';
const paneFor = (id: number) => `${PANE}[data-id="${id}"]`;
const DENSITY = 'button[aria-label="Comfortable rows"]';

const density = () => browser.execute(() => document.documentElement.dataset.density);
const waitForDensity = (want: string) =>
  browser.waitUntil(async () => (await density()) === want, {
    timeoutMsg: `rows never became ${want}`,
  });

describe("rdm GUI power features", () => {
  const ids: number[] = [];

  before(async () => {
    await selectFilter("all");
  });

  after(async () => {
    try {
      await browser.keys(Key.Escape);
      if ((await $(DENSITY).getAttribute("aria-pressed")) === "true") await $(DENSITY).click();
    } finally {
      await removeAll(ids);
    }
  });

  it("offers Delete partial file on a stopped row that kept its partial", async () => {
    const id = await addThroughDialog(`${baseUrl}/slow`);
    ids.push(id);
    const part = await waitForPartBytes(id);
    await stopThroughDialog(id, "keep");
    await waitForStatus(id, "cancelled");
    await waitForAttr(id, "data-partial-kept", "true");

    await openRowMenu(id);
    const menu = $(ROW_MENU);
    await expect(menu.$('button[data-cmd="archive"]')).toBeExisting();
    await expect(menu.$('button[data-cmd="resume"]')).toHaveText("Resume", { containing: true });

    await menu.$('button[data-cmd="deletePartial"]').click();
    await waitForGone(ROW_MENU);
    await waitForAttr(id, "data-partial-kept", "false");
    await browser.waitUntil(async () => !existsSync(part), {
      timeoutMsg: `the partial file ${part} was never deleted`,
    });
    await expect(rowById(id)).toHaveAttribute("data-status", "cancelled");
  });

  it("copies a row's link from the menu", async () => {
    await openRowMenu(ids[0]);
    await $(ROW_MENU).$('button[data-cmd="copyLink"]').click();
    await waitForGone(ROW_MENU);
    // innerText, not getText: toasts may truncate (and more than one may show).
    await browser.waitUntil(
      async () =>
        (
          await browser.execute(
            (sel) =>
              Array.from(document.querySelectorAll<HTMLElement>(sel)).map((t) => t.innerText),
            TOAST,
          )
        ).some((t) => t.includes("Copied the link.")),
      { timeoutMsg: "Copy link never confirmed" },
    );
  });

  it("selects two rows with Ctrl-click and pauses both from the toolbar", async () => {
    const a = await addThroughDialog(`${baseUrl}/crawl`);
    const b = await addThroughDialog(`${baseUrl}/crawl`);
    ids.push(a, b);
    await waitForStatus(a, "downloading");
    await waitForStatus(b, "downloading");

    await clickRow(a);
    await $(paneFor(a)).waitForDisplayed();
    await clickRow(b, { ctrlKey: true });
    // Ctrl-click only selects: the pane stays on the row it showed.
    await expect($(paneFor(a))).toBeDisplayed();
    await expect(rowById(a)).toHaveAttribute("aria-selected", "true");
    await expect(rowById(b)).toHaveAttribute("aria-selected", "true");
    const bar = $(SELECTION_BAR);
    await expect(bar).toHaveText("2 selected", { containing: true });

    await bar.$('button[aria-label="Pause"]').click();
    await waitForStatus(a, "paused");
    await waitForStatus(b, "paused");

    await bar.$('button[aria-label="Clear selection"]').click();
    await waitForGone(SELECTION_BAR);
    await waitForGone(PANE);
    await expect(rowById(a)).toHaveAttribute("aria-selected", "false");
  });

  it("extends the selection with Shift+Down and resumes it with Space", async () => {
    // Newest first, so b sits directly above a.
    const [a, b] = ids.slice(-2);
    await clickRow(b);
    await pressOnTable("ArrowDown", { shiftKey: true });
    await expect(rowById(a)).toHaveAttribute("aria-selected", "true");
    await expect($(SELECTION_BAR)).toHaveText("2 selected", { containing: true });
    // Extending leaves the pane on b.
    await expect($(paneFor(b))).toBeDisplayed();

    await pressOnTable(" ");
    await waitForStatus(a, "downloading");
    await waitForStatus(b, "downloading");

    // A plain arrow moves the cursor, and the open pane follows it.
    await pressOnTable("ArrowUp");
    await pressOnTable("ArrowDown");
    await $(paneFor(a)).waitForDisplayed();

    await pressOnTable("Escape");
    await waitForGone(SELECTION_BAR);
    await waitForGone(PANE);
  });

  it("keeps the details pane on its row when the row leaves the view", async () => {
    const id = await addThroughDialog(`${baseUrl}/crawl`);
    ids.push(id);
    await waitForStatus(id, "downloading");
    await selectFilter("downloading");
    await clickRow(id);
    const pane = `[data-testid="details-pane"][data-id="${id}"]`;
    await $(pane).waitForDisplayed();

    // Paused, it leaves the Downloading view, but the pane stays with it.
    await invoke("pause", { id });
    await waitForGone(rowSelector(id));
    await expect($(pane)).toBeDisplayed();

    await pressOnTable("Escape");
    await waitForGone(pane);
    await selectFilter("all");
  });

  it("switches to comfortable rows and remembers it", async () => {
    const id = ids[0];
    expect(await density()).toBe("compact");
    const compact = (await rectOf(rowSelector(id))).height;

    await $(DENSITY).click();
    await expect($(DENSITY)).toHaveAttribute("aria-pressed", "true");
    await waitForDensity("comfortable");
    await browser.waitUntil(async () => (await rectOf(rowSelector(id))).height > compact, {
      timeoutMsg: "comfortable rows never grew",
    });

    await browser.refresh();
    await $(DENSITY).waitForDisplayed();
    expect(await density()).toBe("comfortable");

    await $(DENSITY).click();
    await waitForDensity("compact");
  });

  it("folds the sidebar into an icon rail in a narrow window", async () => {
    const all = $('button[data-testid="filter"][data-filter="all"]');
    const { width, height } = await browser.getWindowSize();
    try {
      await browser.setWindowSize(700, height);
      await browser.waitUntil(async () => !(await all.$(".f-label").isDisplayed()), {
        timeoutMsg: "the filter labels never folded away",
      });
      // The rail's icons keep their names for screen readers.
      await expect(all).toHaveAttribute("aria-label", expect.stringContaining("All"));
      // The total speed fits the rail, now and at its widest number.
      const speedFits = () =>
        browser.execute(() => {
          const aside = document.querySelector("aside")!;
          const speed = document.querySelector<HTMLElement>('[data-testid="total-speed"]')!;
          const num = speed.querySelector<HTMLElement>(".spark-num");
          const fits = () =>
            aside.scrollWidth <= aside.clientWidth &&
            speed.scrollWidth <= speed.clientWidth &&
            speed.getBoundingClientRect().right <= aside.getBoundingClientRect().right;
          const now = fits();
          if (!num) return { now, widest: false };
          const shown = num.textContent;
          num.textContent = "1023.9";
          const widest = fits();
          num.textContent = shown;
          return { now, widest };
        });
      expect(await speedFits()).toEqual({ now: true, widest: true });
    } finally {
      await browser.setWindowSize(width, height);
    }
    await all.$(".f-label").waitForDisplayed();
  });
});
