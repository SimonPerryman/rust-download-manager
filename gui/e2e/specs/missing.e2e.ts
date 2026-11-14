// End-to-end: a finished download whose file is deleted. Open checks the file first and
// flags the row ("File missing" with its icon in place of progress and speed, counted under
// both Completed and Missing); the details banner names the old path; Locate… relinks the
// record to a chosen file (the OS picker is replaced by the RDM_LOCATE_PATH seam) and
// clears the flag; Remove from list drops only the record.

import { existsSync, mkdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import { $, browser, expect } from "@wdio/globals";
import {
  addThroughDialog,
  baseUrl,
  clickRow,
  clickRowAction,
  engineRow,
  filterCount,
  openLog,
  rectOf,
  removeAll,
  rowById,
  rowSelector,
  selectFilter,
  shownIds,
  TOAST,
  waitForAttr,
  waitForGone,
  waitForStatus,
  waitForText,
} from "../helpers";

const PANE = '[data-testid="details-pane"]';
const BANNER = '[data-testid="missing-banner"]';
const OFFER = '[data-testid="same-folder-offer"]';
const SELECTION_BAR = '[data-testid="selection-bar"]';

type Finished = { id: number; oldPath: string; size: number };

/** Adds /file and waits for it to finish. */
async function addFinished(ids: number[]): Promise<Finished> {
  const id = await addThroughDialog(`${baseUrl}/file`);
  ids.push(id);
  await waitForStatus(id, "completed", 30_000);
  const oldPath = (await engineRow(id))!.final_path!;
  expect(existsSync(oldPath)).toBe(true);
  return { id, oldPath, size: statSync(oldPath).size };
}

/** Deletes the file and lets Open's check flag the row. */
async function loseFile({ id, oldPath }: Finished): Promise<void> {
  rmSync(oldPath);
  // Open checks the file right before opening it: it flags the row and opens nothing.
  await clickRowAction(id, "Open");
  await waitForAttr(id, "data-missing", "true", 15_000);
  expect(openLog().some((l) => l.startsWith(`open ${oldPath}`))).toBe(false);
}

/** Adds /file, waits for it to finish, deletes its file and lets Open's check flag the row. */
async function addAndLoseFile(ids: number[]): Promise<Finished> {
  const f = await addFinished(ids);
  await loseFile(f);
  return f;
}

/**
 * Several downloads at once, finished before any file goes: a name is free again once its file
 * is deleted, so one by one they could all be called "file".
 */
async function addAndLoseFiles(ids: number[], count: number): Promise<Finished[]> {
  const all: Finished[] = [];
  for (let i = 0; i < count; i++) all.push(await addFinished(ids));
  for (const f of all) await loseFile(f);
  return all;
}

/** Puts a file named `name` of `size` bytes in `dir`, as if the user had moved it there. */
function putFile(dir: string, name: string, size: number): string {
  mkdirSync(dir, { recursive: true });
  const file = path.join(dir, name);
  writeFileSync(file, Buffer.alloc(size, 1));
  return file;
}

async function toastTexts(): Promise<string[]> {
  return browser.execute(
    (sel) => Array.from(document.querySelectorAll<HTMLElement>(sel)).map((t) => t.innerText),
    TOAST,
  );
}

async function openDetails(id: number): Promise<void> {
  await rowById(id).$(".fn-btn").click();
  await $(`${PANE}[data-id="${id}"]`).waitForDisplayed();
}

describe("rdm GUI missing files", () => {
  const ids: number[] = [];
  const located = process.env.RDM_LOCATE_PATH!;
  const locatedFolder = process.env.RDM_LOCATE_DIR!;

  before(async () => {
    await selectFilter("all");
  });

  after(async () => {
    await selectFilter("all");
    await removeAll(ids);
    rmSync(path.dirname(located), { recursive: true, force: true });
    rmSync(locatedFolder, { recursive: true, force: true });
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

  it("Locate… lines up with a finished row's Open and leaves Archive in its column", async () => {
    const { id: lostId } = await addAndLoseFile(ids);
    // A just-finished row shows the labelled Open button.
    const okId = await addThroughDialog(`${baseUrl}/file`);
    ids.push(okId);
    await waitForStatus(okId, "completed", 30_000);

    const locate = await rectOf(await rowById(lostId).$("button=Locate…"));
    const open = await rectOf(await rowById(okId).$('button[aria-label="Open"]'));
    expect(Math.abs(locate.x - open.x)).toBeLessThan(1);
    const lostArchive = await rectOf(await rowById(lostId).$('button[aria-label="Archive"]'));
    const okArchive = await rectOf(await rowById(okId).$('button[aria-label="Archive"]'));
    expect(Math.abs(lostArchive.x - okArchive.x)).toBeLessThan(1);
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
    // No other missing download has a file in the new folder, so nothing is offered.
    await expect($(OFFER)).not.toBeExisting();
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

  it("offers the other missing downloads found in the new folder, and relinks them on accept", async () => {
    const [a, b] = await addAndLoseFiles(ids, 2);
    // B's file is in the folder A will be relinked into, with its name and exact size.
    const moved = putFile(path.dirname(located), path.basename(b.oldPath), b.size);
    writeFileSync(located, "found again");
    await openDetails(a.id);
    await $(BANNER).$("button=Locate…").click();

    await waitForAttr(a.id, "data-missing", null, 15_000);
    await $(OFFER).waitForDisplayed();
    expect(await $(OFFER).getText()).toContain("1 other missing download");
    // Offering relinks nothing: B is still flagged until the user accepts.
    expect((await engineRow(b.id))!.file_missing).toBe(true);

    await $(OFFER).$("button=Relink it").click();
    await waitForAttr(b.id, "data-missing", null, 15_000);
    await waitForGone(OFFER, "the offer stayed after accepting");
    expect(path.resolve((await engineRow(b.id))!.final_path!)).toBe(path.resolve(moved));
  });

  it("leaves the others missing when the offer is dismissed", async () => {
    const [a, b] = await addAndLoseFiles(ids, 2);
    putFile(path.dirname(located), path.basename(b.oldPath), b.size);
    writeFileSync(located, "found again");
    await openDetails(a.id);
    await $(BANNER).$("button=Locate…").click();
    await waitForAttr(a.id, "data-missing", null, 15_000);
    await $(OFFER).waitForDisplayed();

    await $(OFFER).$('button[aria-label="Dismiss"]').click();
    await waitForGone(OFFER, "the offer stayed after Dismiss");
    expect((await engineRow(b.id))!.file_missing).toBe(true);
  });

  it("Locate folder… relinks every selected missing row found in one folder and reports the rest", async () => {
    const [x, y, z] = await addAndLoseFiles(ids, 3);
    rmSync(locatedFolder, { recursive: true, force: true });
    const fx = putFile(locatedFolder, path.basename(x.oldPath), x.size);
    const fy = putFile(locatedFolder, path.basename(y.oldPath), y.size);
    // Z's name is there but the size is wrong, so it doesn't match.
    putFile(locatedFolder, path.basename(z.oldPath), z.size + 1);

    await clickRow(x.id);
    await clickRow(y.id, { ctrlKey: true });
    await clickRow(z.id, { ctrlKey: true });
    await $(SELECTION_BAR).$('button[aria-label="Locate folder…"]').click();

    await waitForAttr(x.id, "data-missing", null, 15_000);
    await waitForAttr(y.id, "data-missing", null, 15_000);
    expect(path.resolve((await engineRow(x.id))!.final_path!)).toBe(path.resolve(fx));
    expect(path.resolve((await engineRow(y.id))!.final_path!)).toBe(path.resolve(fy));
    expect((await engineRow(z.id))!.file_missing).toBe(true);
    await browser.waitUntil(
      async () =>
        (await toastTexts()).some((t) =>
          t.includes("Relinked 2 downloads. 1 wasn't found in that folder."),
        ),
      { timeoutMsg: "the Locate folder… toast never showed the counts" },
    );
  });
});
