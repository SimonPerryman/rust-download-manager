// End-to-end: the download queue and speed limits. With the concurrency cap
// lowered to 1 in the Settings popover, a second download waits as Queued and
// is promoted as soon as the first one stops streaming; a queued download can
// be paused and carries its speed limit; bad rates are rejected in place.
// Shares the app state dir with the other specs (see wdio.conf.ts), so it
// restores the cap and removes its downloads when done.

import { $, $$, browser, expect } from "@wdio/globals";

const baseUrl = process.env.RDM_E2E_BASE_URL!;

// WebDriver key codes (webdriverio's `Key` isn't a direct dependency here).
const Key = {
  Ctrl: String.fromCharCode(0xe009),
  Enter: String.fromCharCode(0xe007),
  Backspace: String.fromCharCode(0xe003),
  Escape: String.fromCharCode(0xe00c),
};

const rowById = (id: string) => $(`li[data-testid="download-row"][data-id="${id}"]`);
const settingsButton = () => $('button[aria-label="Settings"]');
const concurrencyInput = () => $('input[aria-label="Max concurrent downloads"]');
const globalLimitInput = () => $('input[aria-label="Global speed limit"]');
const addDialog = () => $('[role="dialog"][aria-label="Add download"]');

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
  await input.waitForExist({ reverse: true });

  // Reopening re-reads the settings from the engine.
  await settingsButton().click();
  await expect(concurrencyInput()).toHaveValue(String(n));
  await settingsButton().click();
}

/** Opens the Add dialog and submits `url` (and an optional speed limit). */
async function submitAddDialog(url: string, limit?: string): Promise<void> {
  await $("button=Add download").click();
  const dialog = addDialog();
  await dialog.waitForDisplayed();
  await dialog.$('input[aria-label="URL"]').setValue(url);
  if (limit) await dialog.$('input[aria-label="Speed limit"]').setValue(limit);
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
  async function add(url: string, limit?: string): Promise<string> {
    const before = await rowIds();
    await submitAddDialog(url, limit);
    await addDialog().waitForExist({ reverse: true });
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
    } finally {
      await $("button*=Active").click();
      for (const id of ids) {
        if (await rowById(id).isExisting()) {
          await rowById(id).$('button[aria-label="Cancel"]').click();
        }
      }
      await $("button*=History").click();
      for (const id of ids) {
        await rowById(id).waitForExist({ timeoutMsg: `download ${id} never reached History` });
        await rowById(id).$('button[aria-label="Remove"]').click();
        await rowById(id).waitForExist({ reverse: true });
      }
      await $("button*=Active").click();
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
    const limited = await add(`${baseUrl}/crawl`, "64K");
    await expect(rowById(limited)).toHaveAttribute("data-status", "queued");
    await expect(rowById(limited).$('[data-testid="row-limit"]')).toHaveText("64.0 KB/s", {
      containing: true,
    });

    await rowById(limited).$('button[aria-label="Pause"]').click();
    await waitForStatus(limited, "paused");
    await expect(rowById(running)).toHaveAttribute("data-status", "downloading");
  });

  it("rejects a malformed speed limit in the Add dialog", async () => {
    await submitAddDialog(`${baseUrl}/crawl`, "5X");
    const dialog = addDialog();
    await expect(dialog).toHaveText("invalid rate", { containing: true });
    await expect(dialog).toBeDisplayed();
    await dialog.$("button=Cancel").click();
    await dialog.waitForExist({ reverse: true });
  });

  it("rejects a malformed global speed limit, then clears it", async () => {
    await settingsButton().click();
    const input = globalLimitInput();
    await expect(input).toBeEnabled();

    await retype(input, "5X");
    await expect(input).toHaveAttribute("aria-invalid", "true");
    await expect($('[data-testid="download-settings"]')).toHaveText("invalid rate", {
      containing: true,
    });

    await retype(input, "");
    await expect(input).not.toHaveAttribute("aria-invalid");
    await expect(input).toHaveValue("");
    await settingsButton().click();
    await input.waitForExist({ reverse: true });
    await expect(rowById(running)).toHaveAttribute("data-status", "downloading");
  });

  it("saves uncommitted edits when the popover closes, rejecting a bad one alone", async () => {
    await settingsButton().click();
    await expect(globalLimitInput()).toBeEnabled();

    // The bad limit first: moving focus to the cap blurs (and rejects) it, but
    // its draft stays dirty. The cap is typed last and never blurred, so only
    // the close-time flush can save either.
    await globalLimitInput().click();
    await browser.keys([Key.Ctrl, "a"]);
    await browser.keys("5X");
    await concurrencyInput().click();
    await browser.keys([Key.Ctrl, "a"]);
    await browser.keys("2");

    // Escape unmounts the popover without blurring the field.
    await browser.keys(Key.Escape);
    await concurrencyInput().waitForExist({ reverse: true });

    await settingsButton().click();
    await expect(concurrencyInput()).toHaveValue("2");
    await expect(globalLimitInput()).toHaveValue("");
    await settingsButton().click();
    await concurrencyInput().waitForExist({ reverse: true });
  });
});
