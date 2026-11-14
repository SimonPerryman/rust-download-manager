// End-to-end: a finished download whose file is deleted. Open checks the file first and
// flags the row ("File missing" with its icon in place of progress and speed, counted under
// both Completed and Missing); the details banner names the old path; Locate… relinks the
// record to a chosen file (the OS picker is replaced by the RDM_LOCATE_PATH seam) and
// clears the flag; Remove from list drops only the record.

import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { $, browser, expect } from "@wdio/globals";
import {
  addThroughDialog,
  baseUrl,
  clickRowAction,
  engineRow,
  filterCount,
  openLog,
  removeAll,
  rowById,
  rowSelector,
  selectFilter,
  shownIds,
  waitForAttr,
  waitForGone,
  waitForStatus,
  waitForText,
} from "../helpers";

const PANE = '[data-testid="details-pane"]';
const BANNER = '[data-testid="missing-banner"]';

/** Adds /file, waits for it to finish, deletes its file and lets Open's check flag the row. */
async function addAndLoseFile(ids: number[]): Promise<{ id: number; oldPath: string }> {
  const id = await addThroughDialog(`${baseUrl}/file`);
  ids.push(id);
  await waitForStatus(id, "completed", 30_000);
  const oldPath = (await engineRow(id))!.final_path!;
  expect(existsSync(oldPath)).toBe(true);
  rmSync(oldPath);

  // Open checks the file right before opening it: it flags the row and opens nothing.
  await clickRowAction(id, "Open");
  await waitForAttr(id, "data-missing", "true", 15_000);
  expect(openLog().some((l) => l.startsWith(`open ${oldPath}`))).toBe(false);
  return { id, oldPath };
}

async function openDetails(id: number): Promise<void> {
  await rowById(id).$(".fn-btn").click();
  await $(`${PANE}[data-id="${id}"]`).waitForDisplayed();
}

describe("rdm GUI missing files", () => {
  const ids: number[] = [];
  const located = process.env.RDM_LOCATE_PATH!;

  before(async () => {
    await selectFilter("all");
  });

  after(async () => {
    await selectFilter("all");
    await removeAll(ids);
    rmSync(path.dirname(located), { recursive: true, force: true });
  });

  it("flags the row, counts it under Completed and Missing, and lists it in the Missing filter", async () => {
    const doneBefore = await filterCount("done");
    const missingBefore = await filterCount("missing");
    const { id } = await addAndLoseFile(ids);

    await waitForText(`${rowSelector(id)} [data-testid="missing-flag"]`, "File missing");
    // The flag takes the progress and speed cells' place: no progress text beside it.
    await expect(rowById(id).$("td.c-speed")).not.toBeExisting();
    // Still a finished download: Completed counts it too.
    await browser.waitUntil(async () => (await filterCount("missing")) === missingBefore + 1, {
      timeoutMsg: "the Missing count never rose",
    });
    expect(await filterCount("done")).toBe(doneBefore + 1);

    await selectFilter("missing");
    expect(await shownIds()).toContain(id);
    await selectFilter("done");
    expect(await shownIds()).toContain(id);
    await selectFilter("all");
  });

  it("Locate… relinks the record to the chosen file and clears the flag", async () => {
    const { id, oldPath } = await addAndLoseFile(ids);
    await openDetails(id);
    await waitForText(`${BANNER} [data-testid="missing-path"]`, path.basename(oldPath));

    // The seam stands in for the OS file picker.
    mkdirSync(path.dirname(located), { recursive: true });
    writeFileSync(located, "found again");
    await $(BANNER).$("button=Locate…").click();

    await waitForAttr(id, "data-missing", null, 15_000);
    await waitForGone(BANNER, "the banner stayed after Locate…");
    const row = (await engineRow(id))!;
    expect(path.resolve(row.final_path!)).toBe(path.resolve(located));
    expect(row.status).toBe("completed");
    await waitForText(`${rowSelector(id)} .fn`, path.basename(located));
    // Opening now works: the new file is checked and opened.
    await clickRowAction(id, "Open");
    await browser.waitUntil(() => openLog().includes(`open ${row.final_path}`), {
      timeoutMsg: "Open never reached the relinked file",
    });
  });

  it("Remove from list drops the record and leaves the folder alone", async () => {
    const { id, oldPath } = await addAndLoseFile(ids);
    await openDetails(id);
    await $(BANNER).$("button=Remove from list").click();

    await waitForGone(rowSelector(id), "the row stayed after Remove from list");
    expect(await engineRow(id)).toBeUndefined();
    expect(existsSync(oldPath)).toBe(false);
  });
});
