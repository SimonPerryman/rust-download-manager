// End-to-end: the download queue and speed limits. With the concurrency cap
// lowered to 1 in the Settings popover, a second download waits as Queued and
// is promoted as soon as the first one stops streaming; a queued download can
// be paused and carries its speed limit; a running download's own limit is
// set, rejected and cleared from its row; bad rates are rejected in place; the
// status bar's global limit stays in sync with the Settings popover; Alt+Up
// moves a waiting download to the front of the queue, and it starts next.
// Shares the app state dir with the other specs (see wdio.conf.ts), so it
// restores the cap and removes its downloads when done.

import { $, browser, expect } from "@wdio/globals";
import {
  ADD_DIALOG,
  CONCURRENCY_INPUT,
  Key,
  LINKS,
  baseUrl,
  clickRow,
  clickRowAction,
  dismissOverlays,
  engineRows,
  invoke,
  openAddDialog,
  pressOnTable,
  removeAll,
  retype,
  rowById,
  rowSelector,
  selectFilter,
  setMaxConcurrent,
  settingsButton,
  shownIds,
  waitForAttr,
  waitForGone,
  waitForStatus,
} from "../helpers";

const GLOBAL_LIMIT_INPUT = 'input[aria-label="Global speed limit"]';
const ROW_LIMIT_INPUT = 'input[aria-label="Speed limit"]';
const STATUS_LIMIT_INPUT = 'input[aria-label="Speed limit for all downloads"]';

const concurrencyInput = () => $(CONCURRENCY_INPUT);
const globalLimitInput = () => $(GLOBAL_LIMIT_INPUT);
const addDialog = () => $(ADD_DIALOG);
const statusLimit = () => $('button[data-testid="status-limit"]');

/** Sets the global speed limit (an amount in KB/s; "" = none) through the Settings popover. */
async function setGlobalLimit(amount: string): Promise<void> {
  await settingsButton().click();
  const input = globalLimitInput();
  await expect(input).toBeEnabled();
  await retype(input, amount);
  await settingsButton().click();
  await waitForGone(GLOBAL_LIMIT_INPUT);
}

/** A speed limit as typed: an amount and the unit picked beside it. */
type Rate = { amount: string; unit: "K" | "M" };

/** Opens the Add dialog and submits `url` (and an optional speed limit). */
async function submitAddDialog(url: string, limit?: Rate): Promise<void> {
  const dialog = await openAddDialog();
  await dialog.$(LINKS).setValue(url);
  if (limit) {
    await dialog.$('button[aria-controls="add-more-options"]').click();
    await dialog.$('input[aria-label="Speed limit"]').setValue(limit.amount);
    await dialog.$('select[aria-label="Speed limit unit"]').selectByAttribute("value", limit.unit);
  }
  await dialog.$("button=Add download").click();
}

describe("rdm GUI download queue", () => {
  // Every download this spec adds, for cleanup.
  const ids: number[] = [];

  /** Adds a download through the dialog and returns its id. */
  async function add(url: string, limit?: Rate): Promise<number> {
    const before = new Set((await engineRows()).map((d) => d.id));
    await submitAddDialog(url, limit);
    await waitForGone(ADD_DIALOG);
    let id: number | undefined;
    await browser.waitUntil(
      async () => {
        id = (await engineRows()).map((d) => d.id).find((r) => !before.has(r));
        return id !== undefined;
      },
      { timeoutMsg: `the download of ${url} never appeared` },
    );
    ids.push(id!);
    await rowById(id!).waitForExist();
    return id!;
  }

  before(async () => {
    await selectFilter("all");
  });

  after(async () => {
    // Restore the cap first: a failure part-way must not leave cap=1 in the
    // shared state dir for the specs that follow.
    try {
      if (await addDialog().isExisting()) {
        await addDialog().$("button=Cancel").click();
      }
      if ((await settingsButton().getAttribute("aria-expanded")) === "true") {
        await settingsButton().click();
      }
      await setMaxConcurrent(3);
      await setGlobalLimit("");
    } finally {
      await removeAll(ids);
    }
  });

  let running = 0;

  it("queues a download past the cap and starts it when a slot frees", async () => {
    await setMaxConcurrent(1);

    const first = await add(`${baseUrl}/crawl`);
    const second = await add(`${baseUrl}/crawl`);

    await waitForStatus(first, "downloading");
    await expect(rowById(second)).toHaveAttribute("data-status", "queued");
    // The only waiting download starts next, and says so.
    await expect(rowById(second)).toHaveAttribute("data-next-up", "true");

    await clickRowAction(first, "Pause");
    await waitForStatus(first, "paused");
    await waitForStatus(second, "downloading");
    running = second;
  });

  it("shows a queued download's limit and pauses it without touching the runner", async () => {
    const limited = await add(`${baseUrl}/crawl`, { amount: "64", unit: "K" });
    await expect(rowById(limited)).toHaveAttribute("data-status", "queued");
    await expect(rowById(limited).$('[data-testid="row-limit"]')).toHaveText("64.0 KB/s", {
      containing: true,
    });

    await clickRowAction(limited, "Pause");
    await waitForStatus(limited, "paused");
    await expect(rowById(running)).toHaveAttribute("data-status", "downloading");
  });

  it("keeps the Add dialog's speed limit numeric and rejects zero", async () => {
    await submitAddDialog(`${baseUrl}/crawl`, { amount: "0", unit: "K" });
    const dialog = addDialog();
    await expect(dialog).toHaveText("Enter a speed above 0", { containing: true });
    await expect(dialog).toBeDisplayed();

    // A rejected field under a collapsed "More options" reopens it, so the error has context.
    const more = dialog.$('button[aria-controls="add-more-options"]');
    await more.click();
    await expect(more).toHaveAttribute("aria-expanded", "false");
    await dialog.$("button=Add download").click();
    await expect(more).toHaveAttribute("aria-expanded", "true");
    await expect(dialog).toHaveText("Enter a speed above 0", { containing: true });

    // Letters never reach the box.
    const amount = dialog.$('input[aria-label="Speed limit"]');
    await amount.click();
    await browser.keys([Key.Ctrl, "a"]);
    await browser.keys("5X");
    await expect(amount).toHaveValue("5");
    await dialog.$("button=Cancel").click();
    await waitForGone(ADD_DIALOG);
  });

  it("rejects a zero global speed limit, then clears it", async () => {
    await settingsButton().click();
    const input = globalLimitInput();
    await expect(input).toBeEnabled();

    await retype(input, "0");
    await expect(input).toHaveAttribute("aria-invalid", "true");
    await expect($('[data-testid="download-settings"]')).toHaveText("Enter a speed above 0", {
      containing: true,
    });

    await retype(input, "");
    await expect(input).not.toHaveAttribute("aria-invalid");
    await expect(input).toHaveValue("");
    await settingsButton().click();
    await waitForGone(GLOBAL_LIMIT_INPUT);
    await expect(rowById(running)).toHaveAttribute("data-status", "downloading");
  });

  it("saves uncommitted edits when the popover closes, rejecting a bad one alone", async () => {
    await settingsButton().click();
    await expect(globalLimitInput()).toBeEnabled();

    // The bad limit first: moving focus to the cap blurs (and rejects) it, but
    // its draft stays dirty. The cap is typed last and never blurred, so only
    // the close-time flush can save either. 0.0001 KB/s gets past the GUI's own
    // checks and is rejected by the engine, in its own update.
    // Typed with element send-keys: WebKitGTK's driver drops repeated characters
    // sent as key actions (`browser.keys("0.0001")` arrives as "0.01"). The
    // field is empty here (the previous test cleared it).
    await globalLimitInput().addValue("0.0001");
    await concurrencyInput().click();
    await browser.keys([Key.Ctrl, "a"]);
    await browser.keys("2");

    // Escape unmounts the popover without blurring the field.
    await browser.keys(Key.Escape);
    await waitForGone(CONCURRENCY_INPUT);

    await settingsButton().click();
    await expect(concurrencyInput()).toHaveValue("2");
    await expect(globalLimitInput()).toHaveValue("");
    await settingsButton().click();
    await waitForGone(CONCURRENCY_INPUT);
  });

  it("sets, rejects and clears a running download's own limit from its row", async () => {
    // Its own download (the cap is 2 by now): `running` may finish before this spec ends.
    const own = await add(`${baseUrl}/crawl`);
    await waitForStatus(own, "downloading");
    const row = rowById(own);
    await expect(row).toHaveAttribute("data-limit", "");
    await expect(row.$('[data-testid="row-limit"]')).not.toBeExisting();

    // Set: the gauge action opens the inline editor; the engine applies it live.
    // Above the global limit is allowed, and the editor says the global one wins.
    await setGlobalLimit("16");
    await clickRowAction(own, "Speed limit");
    const amount = row.$('input[aria-label="Speed limit"]');
    await expect(amount).toBeFocused();
    await expect(row).toHaveText("Empty = no limit.", { containing: true });
    await browser.keys("48");
    await expect(row).toHaveText("The global limit is 16.0 KB/s", { containing: true });
    await browser.keys(Key.Enter);
    await expect(row).toHaveAttribute("data-limit", String(48 * 1024));
    // A downloading row's limit button shows the live speed; the limit is in its name.
    await expect(row.$('[data-testid="row-limit"]')).toHaveAttribute(
      "aria-label",
      "Limited to 48.0 KB/s. Change speed limit",
    );
    await expect(row).toHaveAttribute("data-status", "downloading");

    // Reject: zero is refused in place and nothing is applied; Escape drops the edit.
    await row.$('[data-testid="row-limit"]').click();
    await retype(row.$('input[aria-label="Speed limit"]'), "0");
    await expect(row.$('input[aria-label="Speed limit"]')).toHaveAttribute("aria-invalid", "true");
    await expect(row).toHaveText("Enter a speed above 0", { containing: true });
    await browser.keys(Key.Escape);
    await waitForGone(`${rowSelector(own)} ${ROW_LIMIT_INPUT}`);
    await expect(row).toHaveAttribute("data-limit", String(48 * 1024));
    await expect(row.$('button[aria-label="Speed limit"]')).toBeFocused();

    // Clear: "Remove limit" lifts it, from the keyboard too (Enter on it isn't a commit).
    await row.$('[data-testid="row-limit"]').click();
    await expect(row.$('input[aria-label="Speed limit"]')).toBeFocused();
    await browser.keys(Key.Tab); // the unit picker
    await browser.keys(Key.Tab);
    await expect(row.$('button[aria-label="Remove limit"]')).toBeFocused();
    await browser.keys(Key.Enter);
    await expect(row).toHaveAttribute("data-limit", "");
    await expect(row.$('[data-testid="row-limit"]')).not.toBeExisting();
    await expect(row).toHaveAttribute("data-status", "downloading");
    await setGlobalLimit("");
  });

  it("keeps a queued row's open editor through its promotion, then commits on click-away", async () => {
    // A fresh runner under cap 1, so nothing earlier can take the freed slot.
    await setMaxConcurrent(1);
    for (const id of ids) await invoke("pause", { id });
    const runner = await add(`${baseUrl}/crawl`);
    await waitForStatus(runner, "downloading");
    const queued = await add(`${baseUrl}/crawl`);
    const row = rowById(queued);
    await expect(row).toHaveAttribute("data-status", "queued");

    await clickRowAction(queued, "Speed limit");
    const amount = row.$('input[aria-label="Speed limit"]');
    await expect(amount).toBeFocused();
    await browser.keys("32");
    // Tag this exact element: a remount would replace it with an untagged one.
    await browser.execute((el) => el.setAttribute("data-probe", "1"), await amount);

    // Free the slot without touching the page, so the editor keeps focus.
    await invoke("pause", { id: runner });
    await waitForStatus(queued, "downloading");
    const probed = row.$('input[aria-label="Speed limit"][data-probe="1"]');
    await expect(probed).toBeFocused();
    await expect(probed).toHaveValue("32");
    await expect(row).toHaveAttribute("data-limit", "");

    // Focus leaving the editor (no Enter) commits it.
    await browser.execute(() => (document.activeElement as HTMLElement | null)?.blur());
    await expect(row).toHaveAttribute("data-limit", String(32 * 1024));
    await expect(row.$('[data-testid="row-limit"]')).toHaveAttribute(
      "aria-label",
      "Limited to 32.0 KB/s. Change speed limit",
    );
  });

  it("edits the global limit from the status bar, in sync with the Settings popover", async () => {
    await dismissOverlays();
    await expect(statusLimit()).toHaveText("No speed limit", { containing: true });

    // Status bar → engine → Settings popover.
    await statusLimit().click();
    const input = $(STATUS_LIMIT_INPUT);
    await expect(input).toBeFocused();
    await retype(input, "16");
    await waitForGone(STATUS_LIMIT_INPUT);
    await expect(statusLimit()).toHaveText("Limit 16.0 KB/s", { containing: true });

    await settingsButton().click();
    await expect(globalLimitInput()).toHaveValue("16");
    await settingsButton().click();
    await waitForGone(GLOBAL_LIMIT_INPUT);

    // "Remove limit" beside the status bar's editor commits "no limit".
    await statusLimit().click();
    await expect($(STATUS_LIMIT_INPUT)).toBeFocused();
    await $('[data-testid="status-bar"] button[aria-label="Remove limit"]').click();
    await waitForGone(STATUS_LIMIT_INPUT);
    await expect(statusLimit()).toHaveText("No speed limit", { containing: true });

    // Settings popover → status bar (no refetch: the popover reports what it applied).
    await setGlobalLimit("20");
    await expect(statusLimit()).toHaveText("Limit 20.0 KB/s", { containing: true });
    await setGlobalLimit("");
    await expect(statusLimit()).toHaveText("No speed limit", { containing: true });
  });

  it("moves the last waiting download to the front with Alt+Up, and it starts next", async () => {
    await setMaxConcurrent(1);
    for (const id of ids) await invoke("pause", { id });
    const runner = await add(`${baseUrl}/crawl`);
    await waitForStatus(runner, "downloading");
    const a = await add(`${baseUrl}/crawl`);
    const b = await add(`${baseUrl}/crawl`);
    const c = await add(`${baseUrl}/crawl`);
    for (const id of [a, b, c]) await waitForStatus(id, "queued");
    await expect(rowById(a)).toHaveAttribute("data-next-up", "true");

    /** The waiting rows in table order, which follows the engine's queue. */
    const waitingOrder = async () => {
      const queued = new Set(
        (await engineRows()).filter((d) => d.status === "queued").map((d) => d.id),
      );
      return (await shownIds()).filter((id) => queued.has(id));
    };

    await clickRow(c);
    await expect(rowById(c)).toHaveAttribute("aria-selected", "true");
    await pressOnTable("ArrowUp", { altKey: true });
    await browser.waitUntil(async () => (await waitingOrder()).join() === [a, c, b].join(), {
      timeoutMsg: "Alt+Up never moved the last waiting download up one place",
    });
    await pressOnTable("ArrowUp", { altKey: true });
    await waitForAttr(c, "data-next-up", "true");
    await expect(rowById(a)).not.toHaveAttribute("data-next-up");
    expect(await waitingOrder()).toEqual([c, a, b]);

    // Freeing the slot starts the moved download, not the one that was first.
    await invoke("pause", { id: runner });
    await waitForStatus(c, "downloading");
    const status = async (id: number) => (await engineRows()).find((d) => d.id === id)?.status;
    expect(await status(a)).toBe("queued");
    expect(await status(b)).toBe("queued");

    // Dragging b's grip above a puts b first. Pointer events are dispatched:
    // the drag listens on the window, and drivers differ on pointer actions.
    await selectFilter("waiting");
    await browser.execute(
      (from, onto) => {
        const grip = document.querySelector(`${from} .grip`) as HTMLElement;
        const start = grip.getBoundingClientRect();
        const target = (document.querySelector(onto) as HTMLElement).getBoundingClientRect();
        const at = (y: number) => ({
          bubbles: true,
          clientX: start.left + 4,
          clientY: y,
          button: 0,
        });
        grip.dispatchEvent(new PointerEvent("pointerdown", at(start.top + 4)));
        window.dispatchEvent(new PointerEvent("pointermove", at(target.top + 2)));
        window.dispatchEvent(new PointerEvent("pointerup", at(target.top + 2)));
      },
      rowSelector(b),
      rowSelector(a),
    );
    await waitForAttr(b, "data-next-up", "true");
    expect(await waitingOrder()).toEqual([b, a]);

    // The details pane's Move up does the same for the row it shows.
    await clickRow(a);
    const pane = `[data-testid="details-pane"][data-id="${a}"]`;
    await $(pane).waitForDisplayed();
    await $(pane).$("button=Move up").click();
    await waitForAttr(a, "data-next-up", "true");
    expect(await waitingOrder()).toEqual([a, b]);
    await expect($(pane).$("button=Move up")).toHaveAttribute("aria-disabled", "true");
    // The button that just turned itself off keeps focus.
    expect(
      await browser.execute(
        (sel) => document.querySelector(sel)?.contains(document.activeElement) ?? false,
        pane,
      ),
    ).toBe(true);

    await pressOnTable("Escape");
    await waitForGone('[data-testid="details-pane"]');
    await selectFilter("all");
  });
});
