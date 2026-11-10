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
export const ADD_DIALOG = '[role="dialog"][aria-label="Add downloads"]';
/** The Add dialog's link list (one link per line). */
export const LINKS = 'textarea[aria-label="Links"]';
export const DEST = 'input[aria-label="Destination folder"]';
export const TABLE = 'table[aria-label="Downloads"]';
export const ROW_MENU = '[data-testid="row-menu"]';
export const STOP_DIALOG = '[data-testid="stop-dialog"]';
export const NOTIFICATION = '[data-testid="finish-notification"]';
export const TOAST = '[data-testid="toast"]';
export const UNDO_TOAST = '[data-testid="undo-toast"]';
export const CONCURRENCY_INPUT = 'input[aria-label="Concurrent downloads"]';

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
  url: string;
  filename: string;
  target: string;
  final_path: string | null;
  status: string;
  partial_kept: boolean;
  finished_at: number | null;
  archived: boolean;
  tags: string[];
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

/** An element's layout box in CSS px, as `getBoundingClientRect()` reports it. */
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * The layout box of the first match of the CSS selector (or of an element),
 * read by the page. Not getSize()/getLocation(): WebKitGTK's driver answers
 * the element-rect command with "element not interactable" for elements like
 * table rows. Throws when nothing matches.
 */
export async function rectOf(target: string | WebdriverIO.Element): Promise<Rect> {
  const rect = await browser.execute(
    (sel: string | null, el: Element | null) => {
      const node = sel === null ? el : document.querySelector(sel);
      if (!node) return null;
      const { x, y, width, height } = node.getBoundingClientRect();
      return { x, y, width, height };
    },
    typeof target === "string" ? target : null,
    // Element arguments reach the page as DOM nodes (WebDriver element references).
    (typeof target === "string" ? null : target) as unknown as Element | null,
  );
  if (!rect) {
    const what = typeof target === "string" ? target : String(target.selector);
    throw new Error(`rectOf: nothing matches ${what}`);
  }
  return rect;
}

/**
 * Waits until the first match of the CSS `selector` is displayed and its
 * rendered text (innerText) contains `text`. Not getText(): WebKitGTK's driver
 * returns "" for a displayed Tailwind `truncate` element (the toolbar title,
 * the notification filename), and drops that text from an ancestor's read too.
 * A fresh query each poll, and a thrown error means "not yet", so a re-render
 * replacing the element mid-check can't strand a stale reference. On timeout
 * the error says what the driver and the page saw.
 */
export async function waitForText(
  selector: string,
  text: string,
  timeoutMsg?: string,
): Promise<void> {
  try {
    await browser.waitUntil(
      async () => {
        try {
          if (!(await $(selector).isDisplayed())) return false;
          const shown = await browser.execute(
            (sel) => (document.querySelector(sel) as HTMLElement | null)?.innerText ?? "",
            selector,
          );
          return shown.includes(text);
        } catch {
          return false;
        }
      },
      { timeoutMsg: timeoutMsg ?? `${selector} never showed "${text}"` },
    );
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    throw new Error(`${message}\n${await textDiagnostics(selector)}`, { cause: e });
  }
}

/** What the driver and the page say about `selector`, for a failed text wait. */
async function textDiagnostics(selector: string): Promise<string> {
  const safe = async <T>(f: () => Promise<T>): Promise<T | string> => {
    try {
      return await f();
    } catch (e) {
      return `error: ${String(e).slice(0, 120)}`;
    }
  };
  const el = $(selector);
  // Element commands on a missing element each wait out waitforTimeout, which
  // could push the spec past mocha's timeout and hide this error behind it.
  const exists = await safe(async () => (await $$(selector).length) > 0);
  const driver =
    exists === true
      ? {
          getText: await safe(() => el.getText()),
          isDisplayed: await safe(() => el.isDisplayed()),
        }
      : null;
  const report = {
    exists,
    driver,
    windowSize: await safe(() => browser.getWindowSize()),
    page: await safe(() =>
      browser.execute((sel) => {
        const node = document.querySelector(sel);
        if (!node) return null;
        const cs = getComputedStyle(node);
        const r = node.getBoundingClientRect();
        const chain: string[] = [];
        for (let a = node.parentElement; a; a = a.parentElement) {
          const s = getComputedStyle(a);
          if (s.opacity !== "1" || s.visibility !== "visible" || s.display === "none") {
            chain.push(`${a.tagName}.${a.className}: ${s.display}/${s.visibility}/${s.opacity}`);
          }
        }
        return {
          textContent: node.textContent,
          innerText: (node as HTMLElement).innerText,
          rect: { x: r.x, y: r.y, w: r.width, h: r.height },
          scrollW: node.scrollWidth,
          clientW: node.clientWidth,
          style: {
            display: cs.display,
            visibility: cs.visibility,
            opacity: cs.opacity,
            overflow: cs.overflow,
            textOverflow: cs.textOverflow,
            whiteSpace: cs.whiteSpace,
          },
          ancestorsHidden: chain,
          viewport: { w: window.innerWidth, h: window.innerHeight },
          screen: { w: screen.width, h: screen.height },
        };
      }, selector),
    ),
  };
  return `text diagnostics for ${selector}: ${JSON.stringify(report)}`;
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
 * Waits for the open Add dialog's destination to settle, then sets it to `dir`.
 * It starts at the folder the last add used, or else is filled with the OS
 * download dir asynchronously (a late fill would overwrite a typed path).
 */
export async function setDestination(dir: string = downloadDir): Promise<void> {
  const dest = $(ADD_DIALOG).$(DEST);
  await browser.waitUntil(async () => (await dest.getValue()) !== "", {
    timeoutMsg: "the Add dialog never filled its destination",
  });
  if ((await dest.getValue()) !== dir) await retype(dest, dir, { enter: false });
  await expect(dest).toHaveValue(dir);
}

/** Opens the Add dialog from the toolbar with its destination set to `dir`. */
export async function openAddDialog(dir: string = downloadDir): Promise<ReturnType<typeof $>> {
  await $('[data-testid="add-button"]').click();
  const dialog = $(ADD_DIALOG);
  await dialog.waitForDisplayed();
  await setDestination(dir);
  return dialog;
}

/** Ids the engine has now that aren't in `before`, waiting until there are `n`. */
export async function newIds(before: ReadonlySet<number>, n: number): Promise<number[]> {
  let fresh: number[] = [];
  await browser.waitUntil(
    async () => {
      fresh = (await engineRows()).map((d) => d.id).filter((i) => !before.has(i));
      return fresh.length >= n;
    },
    { timeoutMsg: `${n} new downloads never appeared` },
  );
  return fresh.sort((a, b) => a - b);
}

/**
 * Opens the Add dialog from the toolbar, submits `url` (into `dir`, default
 * the download dir) and returns the new download's id.
 */
export async function addThroughDialog(url: string, dir?: string): Promise<number> {
  const before = new Set((await engineRows()).map((d) => d.id));
  const dialog = await openAddDialog(dir);
  await dialog.$(LINKS).setValue(url);
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

const tagButton = (tag: string) => $(`button[data-testid="tag-filter"][data-tag="${tag}"]`);

/** Selects a sidebar tag filter and waits for it to be the current one. */
export async function selectTag(tag: string): Promise<void> {
  const button = tagButton(tag);
  await button.click();
  await expect(button).toHaveAttribute("aria-current", "true");
}

/** A tag filter's live count, or null while the Tags section doesn't list it. */
export async function tagCount(tag: string): Promise<number | null> {
  const button = tagButton(tag);
  if (!(await button.isExisting())) return null;
  return Number(await button.getAttribute("data-count"));
}

/** The tags the Tags section lists now, in order. */
export async function listedTags(): Promise<string[]> {
  return browser.execute(() =>
    Array.from(document.querySelectorAll('[data-testid="tag-filter"]')).map(
      (b) => b.getAttribute("data-tag") ?? "",
    ),
  );
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

/**
 * Dispatches a keydown on the downloads table (focusing it first). Modifiers go
 * on the event: WebKitGTK's driver drops modifier keys sent as key actions.
 */
export async function pressOnTable(
  key: string,
  mods: { altKey?: boolean; shiftKey?: boolean; ctrlKey?: boolean } = {},
): Promise<void> {
  await browser.execute(
    (sel, k, m) => {
      const table = document.querySelector(sel) as HTMLElement;
      table.focus();
      table.dispatchEvent(
        new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...m }),
      );
    },
    TABLE,
    key,
    mods,
  );
}

/**
 * Clicks a row outside its controls (its size cell), selecting it; a plain
 * click also opens its details. Modifiers go on a dispatched event:
 * WebKitGTK's driver drops modifier keys held during a click.
 */
export async function clickRow(
  id: number,
  mods: { ctrlKey?: boolean; shiftKey?: boolean } = {},
): Promise<void> {
  await dismissOverlays();
  if (!mods.ctrlKey && !mods.shiftKey) {
    await rowById(id).$("td.c-size").click();
    return;
  }
  await browser.execute(
    (sel, m) => {
      const cell = document.querySelector(`${sel} td.c-size`) as HTMLElement;
      cell.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ...m }));
    },
    rowSelector(id),
    mods,
  );
}

/** Opens the right-click menu on a row (a dispatched contextmenu event at its name). */
export async function openRowMenu(id: number): Promise<void> {
  await dismissOverlays();
  await browser.execute((sel) => {
    const cell = document.querySelector(`${sel} td.c-name .fn`) as HTMLElement;
    const r = cell.getBoundingClientRect();
    cell.dispatchEvent(
      new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
        clientX: r.left + 8,
        clientY: r.top + r.height / 2,
      }),
    );
  }, rowSelector(id));
  await $(ROW_MENU).waitForDisplayed();
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

/** The lines `open_download` / `reveal_download` appended to `RDM_OPEN_LOG` (missing file = none). */
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
