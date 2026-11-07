// End-to-end: the finish moment. A download that completes while the app is
// watching stays where it was in All, lingers with a solid Open button and a
// notification, and bumps Done's count; opening it once (from the row or the
// notification) ends the linger, and what "opened" is checked in the
// RDM_OPEN_LOG file instead of launching a program. An unopened row keeps its
// linger across a reload, but a reload never replays the moment itself.

import path from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import {
  NOTIFICATION,
  ROW,
  addThroughDialog,
  attr,
  baseUrl,
  clickOverlay,
  clickRowAction,
  downloadDir,
  engineRow,
  filterCount,
  notificationSelector,
  openLog,
  removeAll,
  rowById,
  selectFilter,
  shownIds,
  waitForAttr,
  waitForGone,
  rowSelector,
  waitForStatus,
  waitForText,
} from "../helpers";

const DONE_COUNT = 'button[data-testid="filter"][data-filter="done"] [data-testid="filter-count"]';

/** The path the GUI opens for a finished download, as the engine reports it. */
async function openedPath(id: number): Promise<string> {
  const d = await engineRow(id);
  if (!d) throw new Error(`download ${id} is not in the engine's list`);
  return d.final_path ?? d.target;
}

/** Waits for `open <path of id>` in the open log, and checks it's in the download dir. */
async function expectOpened(id: number): Promise<void> {
  const opened = await openedPath(id);
  const filename = (await attr(id, "data-filename"))!;
  expect(path.resolve(opened)).toBe(path.resolve(downloadDir, filename));
  await browser.waitUntil(() => openLog().includes(`open ${opened}`), {
    timeoutMsg: `"open ${opened}" never reached the open log`,
  });
}

describe("rdm GUI finish moment", () => {
  const ids: number[] = [];

  after(async () => {
    await removeAll(ids);
  });

  it("keeps a finished download in place, lingering with Open, a notification and Done's bump", async () => {
    await selectFilter("all");
    const doneBefore = await filterCount("done");

    // Watch the Done count element for the bump class from here on: the bump is
    // short, so a single read after the finish could miss it.
    await browser.execute((sel) => {
      const el = document.querySelector(sel)!;
      const w = window as unknown as { __rdmBumpSeen: boolean };
      w.__rdmBumpSeen = false;
      new MutationObserver(() => {
        if (el.classList.contains("bump")) w.__rdmBumpSeen = true;
      }).observe(el, { attributes: true, attributeFilter: ["class"] });
    }, DONE_COUNT);

    const a = await addThroughDialog(`${baseUrl}/slow`);
    ids.push(a);
    await waitForStatus(a, "downloading");
    const index = (await shownIds()).indexOf(a);
    expect(index).toBeGreaterThanOrEqual(0);

    await waitForStatus(a, "completed", 30_000);
    expect((await shownIds()).indexOf(a)).toBe(index);
    await waitForAttr(a, "data-linger", "true");
    // The ~6 s success tint is running (checked well inside its hold).
    await expect(rowById(a)).toHaveElementClass("fresh");
    await waitForText(`${rowSelector(a)} button[aria-label="Open"]`, "Open");

    const card = notificationSelector(a);
    await waitForText(card, "Download finished");
    await waitForText(card, (await attr(a, "data-filename"))!);

    await browser.waitUntil(async () => (await filterCount("done")) === doneBefore + 1, {
      timeoutMsg: `Done's count never went from ${doneBefore} to ${doneBefore + 1}`,
    });
    expect(
      await browser.execute(() => (window as unknown as { __rdmBumpSeen: boolean }).__rdmBumpSeen),
    ).toBe(true);

    await selectFilter("done");
    await rowById(a).waitForExist({ timeoutMsg: `download ${a} isn't shown under Done` });
    await selectFilter("all");
  });

  it("ends the linger once the row's Open is used", async () => {
    const a = ids[0];
    await clickRowAction(a, "Open");
    await waitForAttr(a, "data-linger", null);
    await expectOpened(a);
    // Back to the quiet done state: Open is an icon button now.
    await browser.waitUntil(
      async () => (await rowById(a).$('button[aria-label="Open"]').getText()).trim() === "",
      { timeoutMsg: `download ${a}'s Open button kept its text label` },
    );
  });

  it("ends the linger once the notification's Open is used", async () => {
    const b = await addThroughDialog(`${baseUrl}/file`);
    ids.push(b);
    await waitForStatus(b, "completed", 30_000);
    await $(notificationSelector(b)).waitForExist();

    await clickOverlay(notificationSelector(b), "button=Open");
    await waitForGone(notificationSelector(b));
    await waitForAttr(b, "data-linger", null);
    await expectOpened(b);
  });

  it("keeps a revealed-but-unopened row lingering across a reload without replaying the moment", async () => {
    const c = await addThroughDialog(`${baseUrl}/file`);
    ids.push(c);
    await waitForStatus(c, "completed", 30_000);
    await waitForAttr(c, "data-linger", "true");
    await $(notificationSelector(c)).waitForExist();
    // Show in folder reveals the file and closes the card, but doesn't count as opening it.
    const revealed = await openedPath(c);
    await clickOverlay(notificationSelector(c), "button=Show in folder");
    await waitForGone(notificationSelector(c));
    await browser.waitUntil(() => openLog().includes(`reveal ${revealed}`), {
      timeoutMsg: `"reveal ${revealed}" never reached the open log`,
    });
    await expect(rowById(c)).toHaveAttribute("data-linger", "true");

    await browser.refresh();
    await rowById(c).waitForExist({ timeoutMsg: `download ${c} didn't come back after a reload` });
    await expect(rowById(c)).toHaveAttribute("data-status", "completed");
    // Give a wrongly replayed finish time to render before checking for one.
    await browser.pause(500);
    await expect(rowById(c)).toHaveAttribute("data-linger", "true");
    expect(await $$(NOTIFICATION).length).toBe(0);
    expect(await $$(`${ROW}.fresh`).length).toBe(0);
    await expect($(DONE_COUNT)).not.toHaveElementClass("bump");
  });
});
