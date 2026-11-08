// End-to-end: batch add. Text pasted anywhere outside a text field opens the
// Add dialog with every pasted line; the preview counts the links and skips
// the line that isn't one; Add starts one download per link. The dialog
// starts at the folder the last add used.
// Shares the app state dir with the other specs (see wdio.conf.ts), so it
// removes its downloads and forgets the remembered folder when done, and saves
// into folders of its own so later specs' filenames stay free.

import { mkdirSync } from "node:fs";
import path from "node:path";
import { $, browser, expect } from "@wdio/globals";
import {
  ADD_DIALOG,
  DEST,
  LINKS,
  baseUrl,
  downloadDir,
  engineRows,
  newIds,
  openAddDialog,
  removeAll,
  rowById,
  selectFilter,
  setDestination,
  waitForGone,
} from "../helpers";

/** Where the Add dialog keeps the last folder used (see gui/src/lib/prefs.ts). */
const LAST_DIR_KEY = "rdm.gui.lastDir";

describe("rdm GUI batch add", () => {
  const ids: number[] = [];
  const batchDir = path.join(downloadDir, "batch");
  const other = path.join(downloadDir, "batch-last");

  before(async () => {
    mkdirSync(batchDir, { recursive: true });
    mkdirSync(other, { recursive: true });
    await selectFilter("all");
  });

  after(async () => {
    try {
      if (await $(ADD_DIALOG).isExisting()) await $(ADD_DIALOG).$("button=Cancel").click();
      await browser.execute((key) => localStorage.removeItem(key), LAST_DIR_KEY);
    } finally {
      await removeAll(ids);
    }
  });

  it("previews pasted lines and adds only the links", async () => {
    const first = `${baseUrl}/file`;
    const second = `${baseUrl}/named`;
    const before = new Set((await engineRows()).map((d) => d.id));

    await browser.execute((text) => {
      const data = new DataTransfer();
      data.setData("text/plain", text);
      document.body.dispatchEvent(
        new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
      );
    }, [first, "release notes from the forum", second].join("\n"));

    const dialog = $(ADD_DIALOG);
    await dialog.waitForDisplayed();
    await expect(dialog.$(LINKS)).toHaveValue(
      [first, "release notes from the forum", second].join("\n"),
    );
    const summary = dialog.$('[data-testid="preview-summary"]');
    await expect(summary).toHaveText("2 links found", { containing: true });
    await expect(summary).toHaveText("1 line isn't a link, skipped", { containing: true });
    await expect(
      dialog.$$('[data-testid="link-preview"] li[data-kind="bad"]'),
    ).toBeElementsArrayOfSize(1);

    await setDestination(batchDir);
    const add = dialog.$("button=Add 2 downloads");
    await expect(add).toBeEnabled();
    await add.click();
    await waitForGone(ADD_DIALOG);

    const added = await newIds(before, 2);
    ids.push(...added);
    expect(added).toHaveLength(2);
    const urls = (await engineRows()).filter((d) => added.includes(d.id)).map((d) => d.url);
    expect(urls.sort()).toEqual([first, second].sort());
    for (const id of added) await rowById(id).waitForExist();
  });

  it("starts at the folder the last add used", async () => {
    const before = new Set((await engineRows()).map((d) => d.id));
    const dialog = await openAddDialog(other);
    await dialog.$(LINKS).setValue(`${baseUrl}/file`);
    await dialog.$("button=Add download").click();
    await waitForGone(ADD_DIALOG);
    ids.push(...(await newIds(before, 1)));

    await $('[data-testid="add-button"]').click();
    await $(ADD_DIALOG).waitForDisplayed();
    await expect($(ADD_DIALOG).$(DEST)).toHaveValue(other);
    await $(ADD_DIALOG).$("button=Cancel").click();
    await waitForGone(ADD_DIALOG);
  });
});
