// End-to-end: a link whose file is already in the destination folder. The Add
// dialog's preview flags it with a chip and leaves it un-ticked, so an Add
// starts nothing; Download anyway re-ticks it and the download runs.
// Saves into a folder of its own (with the file planted there) so other specs'
// filenames stay free; removes its downloads when done.

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { $, browser, expect } from "@wdio/globals";
import {
  ADD_DIALOG,
  LINKS,
  baseUrl,
  downloadDir,
  engineRows,
  newIds,
  openAddDialog,
  openLog,
  removeAll,
  rowById,
  selectFilter,
  waitForGone,
} from "../helpers";

describe("rdm GUI already-downloaded link", () => {
  const ids: number[] = [];
  const dir = path.join(downloadDir, "existing");
  // The fixture answers a HEAD here with the payload's size, and the URL names the file.
  const url = `${baseUrl}/probe/any/existing.bin`;
  const planted = path.join(dir, "existing.bin");

  before(async () => {
    rmSync(dir, { recursive: true, force: true });
    mkdirSync(dir, { recursive: true });
    writeFileSync(planted, Buffer.from(await (await fetch(url)).arrayBuffer()));
    await selectFilter("all");
  });

  after(async () => {
    try {
      if (await $(ADD_DIALOG).isExisting()) await $(ADD_DIALOG).$("button=Cancel").click();
    } finally {
      await removeAll(ids);
    }
  });

  it("flags the link, un-ticked, and re-ticking it downloads it", async () => {
    const before = new Set((await engineRows()).map((d) => d.id));
    const dialog = await openAddDialog(dir);
    await dialog.$(LINKS).setValue(url);

    const row = dialog.$('li[data-kind="link"]');
    await row.$('[data-testid="on-disk-chip"]').waitForDisplayed();
    await expect(row).toHaveAttribute("data-ticked", "false");
    // Nothing is ticked, so there is nothing to add.
    await expect(dialog.$("button=Add download")).toBeDisabled();

    await row.$("button=Show in folder").click();
    await browser.waitUntil(() => openLog().some((l) => l === `reveal ${planted}`), {
      timeoutMsg: "Show in folder never revealed the existing file",
    });

    await row.$("button=Download anyway").click();
    await expect(row).toHaveAttribute("data-ticked", "true");
    const add = dialog.$("button=Add download");
    await expect(add).toBeEnabled();
    await add.click();
    await waitForGone(ADD_DIALOG);

    const [id] = await newIds(before, 1);
    ids.push(id);
    await rowById(id).waitForExist();
    expect((await engineRows()).find((d) => d.id === id)?.url).toBe(url);
  });

  it("does not flag a link whose file is not there", async () => {
    const dialog = await openAddDialog(path.join(downloadDir, "existing-empty"));
    await dialog.$(LINKS).setValue(url);
    await dialog.$('li[data-kind="link"]').waitForDisplayed();
    // The check settles after a short delay; give it a moment to (not) flag.
    await browser.pause(1500);
    await expect(dialog.$('[data-testid="on-disk-chip"]')).not.toBeExisting();
    await expect(dialog.$("button=Add download")).toBeEnabled();
    await dialog.$("button=Cancel").click();
    await waitForGone(ADD_DIALOG);
  });
});
