// End-to-end: the download queue and speed limits. With the concurrency cap
// lowered to 1 in the Settings popover, a second download waits as Queued and
// is promoted as soon as the first one stops streaming; a queued download can
// be paused and carries its speed limit; a running download's own limit is
// set, rejected and cleared from its row; bad rates are rejected in place.
// Shares the app state dir with the other specs (see wdio.conf.ts), so it
// restores the cap and removes its downloads when done.

import { $, $$, browser, expect } from "@wdio/globals";
import { waitForGone } from "../helpers";

const baseUrl = process.env.RDM_E2E_BASE_URL!;

// WebDriver key codes (webdriverio's `Key` isn't a direct dependency here).
const Key = {
  Ctrl: String.fromCharCode(0xe009),
  Enter: String.fromCharCode(0xe007),
  Backspace: String.fromCharCode(0xe003),
  Tab: String.fromCharCode(0xe004),
  Escape: String.fromCharCode(0xe00c),
};

const rowSelector = (id: string) => `li[data-testid="download-row"][data-id="${id}"]`;
const CONCURRENCY_INPUT = 'input[aria-label="Max concurrent downloads"]';
const GLOBAL_LIMIT_INPUT = 'input[aria-label="Global speed limit"]';
const ADD_DIALOG = '[role="dialog"][aria-label="Add download"]';
const ROW_LIMIT_INPUT = 'input[aria-label="Speed limit"]';

const rowById = (id: string) => $(rowSelector(id));
const settingsButton = () => $('button[aria-label="Settings"]');
const concurrencyInput = () => $(CONCURRENCY_INPUT);
const globalLimitInput = () => $(GLOBAL_LIMIT_INPUT);
const addDialog = () => $(ADD_DIALOG);

/** Replaces a React-controlled input's text and commits it with Enter. */
async function retype(input: ReturnType<typeof $>, text: string): Promise<void> {
  // Select-and-type rather than setValue: clearing a React-controlled input
  // without an input event gets undone on the next render.
  await input.click();
  await browser.keys([Key.Ctrl, "a"]);
  await browser.keys(text === "" ? [Key.Backspace, Key.Enter] : [text, Key.Enter]);
}

async function waitForStatus(id: string, status: string): Promise<void> {
  await browser.waitUntil(
    async () => {
      const r = rowById(id);
      return (await r.isExisting()) && (await r.getAttribute("data-status")) === status;
    },
    { timeoutMsg: `download ${id} never reached status "${status}"` },
  );
}

/** Sets the concurrency cap through the Settings popover and checks the engine kept it. */
async function setMaxConcurrent(n: number): Promise<void> {
  await settingsButton().click();
  const input = concurrencyInput();
  await expect(input).toBeEnabled();
  await retype(input, String(n));
  await settingsButton().click();
  await waitForGone(CONCURRENCY_INPUT);

  // Reopening re-reads the settings from the engine.
  await settingsButton().click();
  await expect(concurrencyInput()).toHaveValue(String(n));
  await settingsButton().click();
}

/**
 * Switches list tab. Scrolls to the top first: with a long list, WebDriver
 * scrolls the tab to the window's top edge, under the sticky header.
 */
async function showTab(name: "Active" | "History"): Promise<void> {
  await browser.execute(() => window.scrollTo(0, 0));
  await $(`button*=${name}`).click();
}

/** Sets the global speed limit (an amount in KB/s; "" = none) through the Settings popover. */
async function setGlobalLimit(amount: string): Promise<void> {
  await settingsButton().click();
  const input = globalLimitInput();
  await expect(input).toBeEnabled();
  await retype(input, amount);
  await settingsButton().click();
  await waitForGone(GLOBAL_LIMIT_INPUT);
}

/** Calls a backend command directly, without touching the page (so focus stays put). */
async function invoke(cmd: string, args: Record<string, unknown>): Promise<void> {
  await browser.execute(
    (cmd, args) =>
      (
        window as unknown as {
          __TAURI_INTERNALS__: { invoke: (c: string, a: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__.invoke(cmd, args),
    cmd,
    args,
  );
}

/** A speed limit as typed: an amount and the unit picked beside it. */
type Rate = { amount: string; unit: "K" | "M" };

/** Opens the Add dialog and submits `url` (and an optional speed limit). */
async function submitAddDialog(url: string, limit?: Rate): Promise<void> {
  await $("button=Add download").click();
  const dialog = addDialog();
  await dialog.waitForDisplayed();
  await dialog.$('input[aria-label="URL"]').setValue(url);
  if (limit) {
    await dialog.$('button[aria-controls="add-more-options"]').click();
    await dialog.$('input[aria-label="Speed limit"]').setValue(limit.amount);
    await dialog.$('select[aria-label="Speed limit unit"]').selectByAttribute("value", limit.unit);
  }
  await dialog.$("button=Add download").click();
}

/** Ids of every row in the current tab, oldest first. */
async function rowIds(): Promise<string[]> {
  const ids = await $$('li[data-testid="download-row"]').map(
    async (r) => (await r.getAttribute("data-id")) ?? "",
  );
  return ids.sort((a, b) => Number(a) - Number(b));
}

describe("rdm GUI download queue", () => {
  // Every download this spec adds, for cleanup.
  const ids: string[] = [];

  /** Adds a download through the dialog and returns its id. */
  async function add(url: string, limit?: Rate): Promise<string> {
    const before = await rowIds();
    await submitAddDialog(url, limit);
    await waitForGone(ADD_DIALOG);
    let id: string | undefined;
    await browser.waitUntil(
      async () => {
        id = (await rowIds()).find((r) => !before.includes(r));
        return id !== undefined;
      },
      { timeoutMsg: `the download of ${url} never appeared` },
    );
    ids.push(id!);
    return id!;
  }

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
      await showTab("Active");
      for (const id of ids) {
        if (await rowById(id).isExisting()) {
          await rowById(id).$('button[aria-label="Cancel"]').click();
        }
      }
      await showTab("History");
      for (const id of ids) {
        await rowById(id).waitForExist({ timeoutMsg: `download ${id} never reached History` });
        await rowById(id).$('button[aria-label="Remove"]').click();
        await waitForGone(rowSelector(id), `download ${id} was never removed`);
      }
      await showTab("Active");
    }
  });

  let running = "";

  it("queues a download past the cap and starts it when a slot frees", async () => {
    await setMaxConcurrent(1);

    const first = await add(`${baseUrl}/crawl`);
    const second = await add(`${baseUrl}/crawl`);

    await waitForStatus(first, "downloading");
    await expect(rowById(second)).toHaveAttribute("data-status", "queued");
    await expect(rowById(second)).toHaveText("Waiting for a free slot", { containing: true });

    await rowById(first).$('button[aria-label="Pause"]').click();
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

    await rowById(limited).$('button[aria-label="Pause"]').click();
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
    await row.$('button[aria-label="Speed limit"]').click();
    const amount = row.$('input[aria-label="Speed limit"]');
    await expect(amount).toBeFocused();
    await expect(row).toHaveText("Empty = no limit.", { containing: true });
    await browser.keys("48");
    await expect(row).toHaveText("The global limit is 16.0 KB/s", { containing: true });
    await browser.keys(Key.Enter);
    await expect(row).toHaveAttribute("data-limit", String(48 * 1024));
    await expect(row.$('[data-testid="row-limit"]')).toHaveText("48.0 KB/s", { containing: true });
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
    for (const id of ids) await invoke("pause", { id: Number(id) });
    const runner = await add(`${baseUrl}/crawl`);
    await waitForStatus(runner, "downloading");
    const queued = await add(`${baseUrl}/crawl`);
    const row = rowById(queued);
    await expect(row).toHaveAttribute("data-status", "queued");

    await row.$('button[aria-label="Speed limit"]').click();
    const amount = row.$('input[aria-label="Speed limit"]');
    await expect(amount).toBeFocused();
    await browser.keys("32");
    // Tag this exact element: a remount would replace it with an untagged one.
    await browser.execute((el) => el.setAttribute("data-probe", "1"), await amount);

    // Free the slot without touching the page, so the editor keeps focus.
    await invoke("pause", { id: Number(runner) });
    await waitForStatus(queued, "downloading");
    const probed = row.$('input[aria-label="Speed limit"][data-probe="1"]');
    await expect(probed).toBeFocused();
    await expect(probed).toHaveValue("32");
    await expect(row).toHaveAttribute("data-limit", "");

    // Focus leaving the editor (no Enter) commits it.
    await browser.execute(() => (document.activeElement as HTMLElement | null)?.blur());
    await expect(row).toHaveAttribute("data-limit", String(32 * 1024));
    await expect(row.$('[data-testid="row-limit"]')).toHaveText("32.0 KB/s", { containing: true });
  });
});
