// Shared e2e helpers. Rows are found by id (`data-id`) because a restart can
// rename a download (`name (1).ext`); filenames are re-read when needed.

import { existsSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";

export const baseUrl = process.env.RDM_E2E_BASE_URL!;
export const downloadDir = process.env.RDM_DOWNLOAD_DIR!;

// WebDriver key codes (webdriverio's `Key` isn't a direct dependency here).
export const Key = {
  Ctrl: String.fromCharCode(0xe009),
  Shift: String.fromCharCode(0xe008),
  Enter: String.fromCharCode(0xe007),
  Backspace: String.fromCharCode(0xe003),
  Tab: String.fromCharCode(0xe004),
  Escape: String.fromCharCode(0xe00c),
};

export const ROW = 'tr[data-testid="download-row"]';
export const ADD_DIALOG = '[role="dialog"][aria-label="Add download"]';
export const STOP_DIALOG = '[data-testid="stop-dialog"]';
export const NOTIFICATION = '[data-testid="finish-notification"]';
export const TOAST = '[data-testid="toast"]';
export const UNDO_TOAST = '[data-testid="undo-toast"]';
export const CONCURRENCY_INPUT = 'input[aria-label="Max concurrent downloads"]';

export const rowSelector = (id: number) => `${ROW}[data-id="${id}"]`;
export const rowById = (id: number) => $(rowSelector(id));
export const notificationSelector = (id: number) => `${NOTIFICATION}[data-id="${id}"]`;
export const errorRowSelector = (id: number) => `tr[data-testid="download-error"][data-id="${id}"]`;
export const settingsButton = () => $('button[aria-label="Settings"]');

/** The engine statuses that end a download (no task running). */
const TERMINAL = ["completed", "failed", "cancelled"];

/** What the specs read from the engine's `list` (a subset of `DownloadSnapshot`). */
export interface EngineRow {
  id: number;
  filename: string;
  target: string;
  final_path: string | null;
  status: string;
  partial_kept: boolean;
  finished_at: number | null;
}

/** Waits until nothing matches `selector`. */
export async function waitForGone(selector: string, timeoutMsg?: string): Promise<void> {
  await browser.waitUntil(
    async () => {
      // A fresh query each poll, and a thrown error means "not gone yet":
      // WebKitGTK's driver reports an element detached mid-check as a stale
      // element, not as absent, so waitForExist({ reverse: true }) flakes.
      try {
        return (await $$(selector).length) === 0;
      } catch {
        return false;
      }
    },
    { timeoutMsg: timeoutMsg ?? `${selector} never went away` },
  );
}

/**
 * Replaces a React-controlled input's text (and commits it with Enter unless
 * `enter` is false). Select-and-type rather than setValue: clearing without
 * an input event gets undone on the next render.
 */
export async function retype(
  input: ReturnType<typeof $>,
  text: string,
  { enter = true }: { enter?: boolean } = {},
): Promise<void> {
  await input.click();
  await browser.keys([Key.Ctrl, "a"]);
  await browser.keys(Key.Backspace);
  if (text !== "") await input.addValue(text);
  if (enter) await browser.keys(Key.Enter);
}

/** Calls a backend command directly, without touching the page (so focus stays put). */
export async function invoke<T = unknown>(
  cmd: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  const result = await browser.execute(
    (cmd, args) =>
      (
        window as unknown as {
          __TAURI_INTERNALS__: { invoke: (c: string, a: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__.invoke(cmd, args),
    cmd,
    args,
  );
  return result as T;
}

/** Every download as the engine sees it now. */
export const engineRows = () => invoke<EngineRow[]>("list");

/** One download as the engine sees it, or undefined once it's gone. */
export async function engineRow(id: number): Promise<EngineRow | undefined> {
  return (await engineRows()).find((d) => d.id === id);
}

/** The `.part` file a download is writing to right now (re-read: restarts can retarget). */
export async function partPath(id: number): Promise<string> {
  const d = await engineRow(id);
  if (!d) throw new Error(`download ${id} is not in the engine's list`);
  return `${d.target}.part`;
}

/** Waits until the download's `.part` file has some bytes. */
export async function waitForPartBytes(id: number): Promise<string> {
  let part = "";
  await browser.waitUntil(
    async () => {
      part = await partPath(id);
      return existsSync(part) && statSync(part).size > 0;
    },
    { timeoutMsg: `download ${id}'s .part never received any bytes` },
  );
  return part;
}

/** A row attribute, or null when the row or the attribute is absent. */
export async function attr(id: number, name: string): Promise<string | null> {
  const r = rowById(id);
  if (!(await r.isExisting())) return null;
  return r.getAttribute(name);
}

/** Waits for the row of `id` to show engine status `status`. */
export async function waitForStatus(id: number, status: string, timeout?: number): Promise<void> {
  await browser.waitUntil(
    async () => {
      try {
        return (await attr(id, "data-status")) === status;
      } catch {
        return false;
      }
    },
    { timeout, timeoutMsg: `download ${id} never reached status "${status}"` },
  );
}

/** Waits for a row attribute to equal `value` (null = absent). */
export async function waitForAttr(
  id: number,
  name: string,
  value: string | null,
  timeout?: number,
): Promise<void> {
  await browser.waitUntil(
    async () => {
      try {
        const r = rowById(id);
        return (await r.isExisting()) && (await r.getAttribute(name)) === value;
      } catch {
        return false;
      }
    },
    { timeout, timeoutMsg: `download ${id} never had ${name}=${String(value)}` },
  );
}

/** Ids of the rows shown now, in DOM (display) order. */
export async function shownIds(): Promise<number[]> {
  return browser.execute(
    (sel) =>
      Array.from(document.querySelectorAll(sel)).map((r) => Number(r.getAttribute("data-id"))),
    ROW,
  );
}

/**
 * Opens the Add dialog from the toolbar, submits `url` (into `dir` if given)
 * and returns the new download's id.
 */
export async function addThroughDialog(url: string, dir?: string): Promise<number> {
  const before = new Set((await engineRows()).map((d) => d.id));
  await $('[data-testid="add-button"]').click();
  const dialog = $(ADD_DIALOG);
  await dialog.waitForDisplayed();
  // The dialog fills its default destination asynchronously; a late fill
  // would overwrite a typed path, so wait for it first.
  const dest = dialog.$('input[aria-label="Destination folder"]');
  await expect(dest).toHaveValue(downloadDir);
  if (dir !== undefined) {
    await retype(dest, dir, { enter: false });
    await expect(dest).toHaveValue(dir);
  }
  await dialog.$('input[aria-label="URL"]').setValue(url);
  await dialog.$("button=Add download").click();
  await waitForGone(ADD_DIALOG);

  let id: number | undefined;
  await browser.waitUntil(
    async () => {
      const fresh = (await engineRows()).map((d) => d.id).filter((i) => !before.has(i));
      id = fresh.length ? Math.max(...fresh) : undefined;
      return id !== undefined;
    },
    { timeoutMsg: `the download of ${url} never appeared` },
  );
  await rowById(id!).waitForExist({ timeoutMsg: `download ${id} never showed a row` });
  return id!;
}

const filterButton = (key: string) => $(`button[data-testid="filter"][data-filter="${key}"]`);

/** Selects a sidebar status filter and waits for it to be the current one. */
export async function selectFilter(key: string): Promise<void> {
  const button = filterButton(key);
  await button.click();
  await expect(button).toHaveAttribute("aria-current", "true");
}

/** A status filter's live count (assert it inside `waitUntil`, never with one read). */
export async function filterCount(key: string): Promise<number> {
  return Number(await filterButton(key).getAttribute("data-count"));
}

/**
 * The folder filter for `dir`, if listed. Compared in JS, not a CSS attribute
 * selector: Windows backslashes are CSS escapes.
 */
export async function folderButton(dir: string): Promise<WebdriverIO.Element | undefined> {
  const want = path.resolve(dir);
  for (const button of await $$('[data-testid="folder-filter"]')) {
    const got = await button.getAttribute("data-dir");
    if (got && path.resolve(got) === want) return button;
  }
  return undefined;
}

/** True for errors that mean "the element went away" (stale, detached, not found). */
function isGoneError(e: unknown): boolean {
  return /stale|no such element|not exist|detached|not found|not attached/i.test(String(e));
}

/**
 * Clicks `button` (a selector inside the first `container` match) once the
 * overlay has finished sliding in (`data-entered`), then moves the pointer off
 * it so hover-pause can't keep a card alive. Re-queries every poll; if the
 * overlay goes away on its own first, that counts as done.
 */
export async function clickOverlay(container: string, button: string): Promise<void> {
  await browser.waitUntil(
    async () => {
      try {
        const box = $(container);
        if (!(await box.isExisting())) return true;
        if ((await box.getAttribute("data-entered")) !== "true") return false;
        await box.$(button).click();
        return true;
      } catch (e) {
        return isGoneError(e);
      }
    },
    { timeoutMsg: `couldn't click ${button} in ${container}` },
  );
  try {
    await $('[data-testid="status-bar"]').moveTo();
  } catch {
    // Pointer moves are best effort; the click already landed.
  }
}

/**
 * Dismisses every finish notification and info toast (never the undo toast),
 * so none can sit over a row's actions and intercept a click.
 */
export async function dismissOverlays(): Promise<void> {
  for (const selector of [NOTIFICATION, TOAST]) {
    for (let i = 0; i < 10; i++) {
      let count: number;
      try {
        count = await $$(selector).length;
      } catch {
        continue;
      }
      if (count === 0) break;
      await clickOverlay(selector, 'button[aria-label="Dismiss"]');
    }
    await waitForGone(selector, `${selector} never went away after Dismiss`);
  }
}

/** Clicks a row action (by its aria-label), clearing overlays first. */
export async function clickRowAction(id: number, label: string): Promise<void> {
  await dismissOverlays();
  await rowById(id).$(`button[aria-label="${label}"]`).click();
}

/**
 * Stops `id` through the Stop dialog, keeping or deleting the partial file,
 * and waits for the dialog to close (the stop itself may still be landing).
 */
export async function stopThroughDialog(id: number, mode: "keep" | "delete"): Promise<void> {
  await clickRowAction(id, "Stop");
  const dialog = $(STOP_DIALOG);
  await dialog.waitForDisplayed();
  if (mode === "delete") {
    await dialog.$('.//label[.//input[@name="stop-mode" and @value="delete"]]').click();
    await expect(dialog.$('input[name="stop-mode"][value="delete"]')).toBeSelected();
    await dialog.$("button=Stop and delete").click();
  } else {
    await expect(dialog.$('input[name="stop-mode"][value="keep"]')).toBeSelected();
    await dialog.$("button=Stop").click();
  }
  await waitForGone(STOP_DIALOG);
}

/** The lines `open_path` / `reveal_path` appended to `RDM_OPEN_LOG` (missing file = none). */
export function openLog(): string[] {
  const log = process.env.RDM_OPEN_LOG;
  if (!log || !existsSync(log)) return [];
  return readFileSync(log, "utf8")
    .split(/\r?\n/)
    .filter((l) => l !== "");
}

/** Sets the concurrency cap through the Settings popover and checks the engine kept it. */
export async function setMaxConcurrent(n: number): Promise<void> {
  await settingsButton().click();
  const input = $(CONCURRENCY_INPUT);
  await expect(input).toBeEnabled();
  await retype(input, String(n));
  await settingsButton().click();
  await waitForGone(CONCURRENCY_INPUT);

  // Reopening re-reads the settings from the engine.
  await settingsButton().click();
  await expect($(CONCURRENCY_INPUT)).toHaveValue(String(n));
  await settingsButton().click();
  await waitForGone(CONCURRENCY_INPUT);
}

/**
 * Removes downloads from the list through the engine: stops (deleting the
 * partial) anything still running or holding a kept partial, then removes it
 * and waits for its row to go. Ids already gone are skipped.
 */
export async function removeAll(ids: number[]): Promise<void> {
  for (const id of ids) {
    const d = await engineRow(id);
    if (!d) continue;
    if (!TERMINAL.includes(d.status) || (d.status === "cancelled" && d.partial_kept)) {
      await invoke("stop", { id, keepPartial: false });
      await browser.waitUntil(
        async () => {
          const now = await engineRow(id);
          return !now || (TERMINAL.includes(now.status) && !now.partial_kept);
        },
        { timeoutMsg: `download ${id} never stopped` },
      );
    }
    await invoke("remove", { id });
    await waitForGone(rowSelector(id), `download ${id} was never removed`);
  }
}
