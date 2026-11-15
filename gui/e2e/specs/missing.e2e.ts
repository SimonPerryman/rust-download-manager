// End-to-end: a finished download whose file is deleted. Open checks the file first and
// flags the row ("File missing" with its icon in place of progress and speed, counted under
// both Completed and Missing); the details banner names the old path; Locate… relinks the
// record to a chosen file (the OS picker is replaced by the RDM_LOCATE_PATH seam) and
// clears the flag; Remove from list drops only the record. Find file searches nearby folders
// (a match, several candidates, none, and the Log entry), Locate… refuses a file of another size
// inline, Find files reviews many at once, and the same-folder offer's Search reviews its files;
// none of them relinks until the user says so.

import { existsSync, mkdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import {
  addThroughDialog,
  baseUrl,
  clickRow,
  clickRowAction,
  downloadDir,
  engineRow,
  filterCount,
  openLog,
  openRowMenu,
  rectOf,
  ROW_MENU,
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
const REVIEW = '[data-testid="find-review"]';
const reviewRow = (id: number) => `${REVIEW} [data-testid="find-review-row"][data-id="${id}"]`;

type Finished = { id: number; oldPath: string; size: number };

/** Adds a download (default /file) and waits for it to finish. */
async function addFinished(ids: number[], url = `${baseUrl}/file`): Promise<Finished> {
  const id = await addThroughDialog(url);
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
 * A download with its own name and size, finished and then lost: unlike /file, nothing else
 * in the list has its name, so a search can't match more than the file the test put there.
 */
async function addAndLoseNamed(ids: number[], name: string, bytes: number): Promise<Finished> {
  const f = await addFinished(ids, `${baseUrl}/seg/${bytes}/${name}`);
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
  // The pane remembers its tab (an earlier test may have left Log open); the banner is in Overview.
  await $(PANE).$('button[role="tab"][data-tab="overview"]').click();
}

describe("rdm GUI missing files", () => {
  const ids: number[] = [];
  const foundDirs: string[] = [];
  /** A fresh folder inside the download folder, which every search looks in. */
  const foundDir = (tag: string): string => {
    const dir = path.join(downloadDir, `found-${tag}`);
    rmSync(dir, { recursive: true, force: true });
    foundDirs.push(dir);
    return dir;
  };
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
    for (const dir of foundDirs) rmSync(dir, { recursive: true, force: true });
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
    const { id, oldPath, size } = await addAndLoseFile(ids);
    await openDetails(id);
    await waitForText(`${BANNER} [data-testid="missing-path"]`, path.basename(oldPath));

    // The seam stands in for the OS file picker.
    mkdirSync(path.dirname(located), { recursive: true });
    writeFileSync(located, Buffer.alloc(size, 1));
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

  it("offers to search the new folder for the others, and relinks the ones reviewed and ticked", async () => {
    const a = await addAndLoseNamed(ids, "offer-a.bin", 3101);
    const b = await addAndLoseNamed(ids, "offer-b.bin", 3102);
    // B's file is in the folder A will be relinked into, with its name and exact size.
    const moved = putFile(path.dirname(located), path.basename(b.oldPath), b.size);
    writeFileSync(located, Buffer.alloc(a.size, 1));
    await openDetails(a.id);
    await $(BANNER).$("button=Locate…").click();

    await waitForAttr(a.id, "data-missing", null, 15_000);
    await $(OFFER).waitForDisplayed();
    await waitForText(OFFER, "more missing file");
    // Offering relinks nothing: B is still flagged until the user has reviewed it.
    expect((await engineRow(b.id))!.file_missing).toBe(true);

    await $(OFFER).$("button=Search").click();
    await waitForGone(OFFER, "the offer stayed after Search");
    await $(reviewRow(b.id)).waitForDisplayed();
    // Same name and size: ticked, and still not relinked.
    await expect($(`${reviewRow(b.id)} [role="checkbox"]`)).toHaveAttribute("aria-checked", "true");
    expect((await engineRow(b.id))!.file_missing).toBe(true);

    await $(REVIEW).$("button=Relink selected").click();
    await waitForAttr(b.id, "data-missing", null, 15_000);
    expect(path.resolve((await engineRow(b.id))!.final_path!)).toBe(path.resolve(moved));
  });

  it("leaves the others missing when the offer is dismissed", async () => {
    const a = await addAndLoseNamed(ids, "dismiss-a.bin", 3103);
    const b = await addAndLoseNamed(ids, "dismiss-b.bin", 3104);
    putFile(path.dirname(located), path.basename(b.oldPath), b.size);
    writeFileSync(located, Buffer.alloc(a.size, 1));
    await openDetails(a.id);
    await $(BANNER).$("button=Locate…").click();
    await waitForAttr(a.id, "data-missing", null, 15_000);
    await $(OFFER).waitForDisplayed();

    await $(OFFER).$('button[aria-label="Dismiss"]').click();
    await waitForGone(OFFER, "the offer stayed after Dismiss");
    expect((await engineRow(b.id))!.file_missing).toBe(true);
  });

  it("Find file offers one match, relinks only on Relink, and logs the search", async () => {
    const f = await addAndLoseNamed(ids, "find-one.bin", 3201);
    const moved = putFile(foundDir("one"), "find-one.bin", f.size);
    await openDetails(f.id);
    await $(BANNER).$("button=Find file").click();

    await $('[data-testid="find-match"]').waitForDisplayed();
    await waitForText('[data-testid="find-match"]', "Match found");
    await waitForText('[data-testid="find-match"]', "Same name and size");
    // A suggestion only: the record is still missing until Relink.
    expect((await engineRow(f.id))!.file_missing).toBe(true);

    await $('[data-testid="find-match"]').$("button=Relink").click();
    await waitForAttr(f.id, "data-missing", null, 15_000);
    expect(path.resolve((await engineRow(f.id))!.final_path!)).toBe(path.resolve(moved));

    // The search is in the download's log: the folders it looked in and what it found.
    await $(PANE).$('button[role="tab"][data-tab="log"]').click();
    await waitForText(`${PANE} [data-testid="log"]`, "Searched");
    await waitForText(`${PANE} [data-testid="log"]`, "found a match in");
  });

  it("Find file lists candidates when the file is in several folders, and relinks the one picked", async () => {
    const f = await addAndLoseNamed(ids, "find-many.bin", 3202);
    putFile(foundDir("many-a"), "find-many.bin", f.size);
    const second = putFile(foundDir("many-b"), "find-many.bin", f.size);
    await openDetails(f.id);
    await $(BANNER).$("button=Find file").click();

    const panel = '[data-testid="find-candidates"]';
    await waitForText(panel, "2 possible matches found");
    const radios = await $(panel).$$('[role="radio"]');
    expect(radios.length).toBe(2);
    await radios[1].click();
    await expect(radios[1]).toHaveAttribute("aria-checked", "true");
    expect((await engineRow(f.id))!.file_missing).toBe(true);

    await $(panel).$("button=Relink").click();
    await waitForAttr(f.id, "data-missing", null, 15_000);
    expect(path.resolve((await engineRow(f.id))!.final_path!)).toBe(path.resolve(second));
  });

  it("Find file says when nothing matched, and Locate… refuses a file of another size inline", async () => {
    const f = await addAndLoseNamed(ids, "find-none.bin", 3203);
    // Right name, wrong size: not a match.
    putFile(foundDir("none"), "find-none.bin", 3999);
    await openDetails(f.id);
    await $(BANNER).$("button=Find file").click();

    const panel = '[data-testid="find-nomatch"]';
    await waitForText(panel, "No match found");
    await waitForText(panel, "folders searched");
    await waitForText(`${panel} ul`, path.basename(downloadDir));

    // Locate… with a file of another size: refused, with the reason in the panel.
    mkdirSync(path.dirname(located), { recursive: true });
    writeFileSync(located, Buffer.alloc(f.size + 5, 1));
    await $(panel).$("button=Locate…").click();
    await waitForText('[data-testid="locate-error"]', "That file is a different size.");
    expect((await engineRow(f.id))!.file_missing).toBe(true);

    // Search again runs a fresh search.
    putFile(foundDir("none"), "find-none.bin", f.size);
    await $(panel).$("button=Search again").click();
    await $('[data-testid="find-match"]').waitForDisplayed();
  });

  it("Find file is in the row's right-click menu", async () => {
    const f = await addAndLoseNamed(ids, "find-menu.bin", 3204);
    await openRowMenu(f.id);
    await $(ROW_MENU).$("button=Find file").waitForDisplayed();
    await $(ROW_MENU).$("button=Find file").click();
    // The pane opens on the record to show the answer.
    await $(`${PANE}[data-id="${f.id}"]`).waitForDisplayed();
    await $('[data-testid="find-nomatch"]').waitForDisplayed();
  });

  it("Find files is only in the Missing view, and reviews every file it searched for", async () => {
    const x = await addAndLoseNamed(ids, "bulk-x.bin", 3301);
    const y = await addAndLoseNamed(ids, "bulk-y.bin", 3302);
    const z = await addAndLoseNamed(ids, "bulk-z.bin", 3303);
    const dir = foundDir("bulk");
    const fx = putFile(dir, "bulk-x.bin", x.size);
    putFile(dir, "bulk-y.bin", y.size);

    await selectFilter("all");
    await expect($('[data-testid="find-files-button"]')).not.toBeExisting();
    await selectFilter("missing");
    await $('[data-testid="find-files-button"]').click();

    await $(REVIEW).waitForDisplayed();
    await waitForText(`${REVIEW} [data-testid="find-review-heading"]`, "Found ");
    await expect($(`${reviewRow(x.id)} [role="checkbox"]`)).toHaveAttribute("aria-checked", "true");
    await expect($(`${reviewRow(y.id)} [role="checkbox"]`)).toHaveAttribute("aria-checked", "true");
    // No match: nothing to tick, but Locate… is there.
    await expect($(`${reviewRow(z.id)} [role="checkbox"]`)).not.toBeExisting();
    await waitForText(reviewRow(z.id), "Not found");
    await expect($(reviewRow(z.id)).$("button=Locate…")).toBeDisplayed();
    // Un-tick Y: only X is relinked.
    await $(`${reviewRow(y.id)} [role="checkbox"]`).click();
    expect((await engineRow(x.id))!.file_missing).toBe(true);

    await $(REVIEW).$("button=Relink selected").click();
    // Back in the full list: a relinked row leaves the Missing view.
    await selectFilter("all");
    await waitForAttr(x.id, "data-missing", null, 15_000);
    expect(path.resolve((await engineRow(x.id))!.final_path!)).toBe(path.resolve(fx));
    expect((await engineRow(y.id))!.file_missing).toBe(true);
    expect((await engineRow(z.id))!.file_missing).toBe(true);
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
        (await toastTexts()).some((t) => t.includes("Relinked 2. 1 wasn't found in that folder.")),
      { timeoutMsg: "the Locate folder… toast never showed the counts" },
    );

    // Show not found: the Missing view, with exactly the not-found row selected.
    await $(TOAST).$("button=Show not found").click();
    await browser.waitUntil(
      async () => (await rowById(z.id).getAttribute("aria-selected")) === "true",
      { timeoutMsg: "Show not found never selected the not-found row" },
    );
    await expect($(`button[data-testid="filter"][data-filter="missing"]`)).toHaveAttribute(
      "aria-current",
      "true",
    );
    expect(await shownIds()).toContain(z.id);
    // Other missing rows (earlier specs leave some) stay unselected.
    expect(await $$('tr[aria-selected="true"]').length).toBe(1);
    await waitForGone(TOAST, "the toast stayed after Show not found");
    await selectFilter("all");
  });
});
