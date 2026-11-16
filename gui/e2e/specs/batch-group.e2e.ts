// End-to-end: named batches. A pattern prefills the Add dialog's Batch name and
// clearing it undoes the batch; a named batch goes into a folder of its name and
// shows as one group row (progress, "2 of 3 done", Retry failed for that batch
// only); the list can be flat; the sidebar's Batches section filters to one
// batch. The preview marks a link already in the list with an info mark and
// lets "N more" reveal the rest of a range.
// Shares the app state dir with the other specs (see wdio.conf.ts), so it
// removes its downloads and forgets the remembered folder when done, and saves
// into a folder of its own.

import { mkdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import {
  ADD_DIALOG,
  LINKS,
  attr,
  baseUrl,
  downloadDir,
  engineRow,
  engineRows,
  listedTags,
  newIds,
  openAddDialog,
  rectOf,
  removeAll,
  retype,
  selectFilter,
  setDestination,
  shownIds,
  waitForGone,
  waitForStatus,
  waitForText,
} from "../helpers";

const LAST_DIR_KEY = "rdm.gui.lastDir";
const PREFS_KEY = "rdm.gui.prefs";
const BATCH_NAME = 'input[aria-label="Batch name"]';
const FOLDER_TICK = '[data-testid="batch-folder"]';
const GROUP_TOGGLE = 'button[aria-label="Group batches"]';
const BATCH_ROW = 'tr[data-testid="batch-row"]';

/** Pastes `lines` outside any text field, which opens the Add dialog with them. */
async function paste(lines: string[]): Promise<void> {
  await browser.execute((text) => {
    const data = new DataTransfer();
    data.setData("text/plain", text);
    document.body.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
    );
  }, lines.join("\n"));
  await $(ADD_DIALOG).waitForDisplayed();
}

describe("rdm GUI batch groups", () => {
  const ids: number[] = [];
  const root = path.join(downloadDir, "batch-group");
  let batch = 0;
  let items: number[] = [];
  let loose = 0;

  before(async () => {
    mkdirSync(root, { recursive: true });
    await selectFilter("all");
  });

  after(async () => {
    try {
      if (await $(ADD_DIALOG).isExisting()) await $(ADD_DIALOG).$("button=Cancel").click();
      await browser.execute(
        (lastDir, prefs) => {
          localStorage.removeItem(lastDir);
          const stored = JSON.parse(localStorage.getItem(prefs) ?? "{}");
          localStorage.setItem(prefs, JSON.stringify({ ...stored, groupBatches: true }));
        },
        LAST_DIR_KEY,
        PREFS_KEY,
      );
    } finally {
      await removeAll(ids);
    }
  });

  it("fills the batch name from a pattern, and clearing it undoes the batch", async () => {
    await paste([`${baseUrl}/any/lesson[1-3].bin`]);
    const dialog = $(ADD_DIALOG);
    await expect(dialog.$(BATCH_NAME)).toHaveValue("lesson");
    await setDestination(root);
    await expect(dialog.$(FOLDER_TICK)).toBeSelected();
    await expect(dialog.$(FOLDER_TICK).parentElement()).toHaveText("Create folder “lesson” in", {
      containing: true,
    });

    // More options has no checksum for several links, and its limit is per file.
    await dialog.$("button*=More options").click();
    await expect(dialog.$('input[aria-label="SHA-256 checksum"]')).not.toBeExisting();
    await expect(dialog.$("label=Speed limit per file")).toBeExisting();

    await retype(dialog.$(BATCH_NAME), "", { enter: false });
    await waitForGone(`${ADD_DIALOG} ${FOLDER_TICK}`);
    await dialog.$("button=Cancel").click();
    await waitForGone(ADD_DIALOG);
  });

  it("lets N more reveal the rest of a range, and marks a repeat with an info mark", async () => {
    await paste([`${baseUrl}/any/set[01-12].bin`]);
    const dialog = $(ADD_DIALOG);
    const list = '[data-testid="link-preview"] ul';
    const collapsed = (await rectOf(list)).height;

    await dialog.$('button[aria-label^="Show set01.bin"]').click();
    await expect(dialog.$$('li[data-kind="pattern-link"]')).toBeElementsArrayOfSize(3);
    await dialog.$("button=9 more").click();
    await expect(dialog.$$('li[data-kind="pattern-link"]')).toBeElementsArrayOfSize(12);
    await expect(dialog.$("button=Show less")).toBeExisting();
    // Rows a comfortable height, and the list grew to show them.
    expect((await rectOf('li[data-kind="pattern-link"]')).height).toBeGreaterThanOrEqual(32);
    expect((await rectOf(list)).height).toBeGreaterThan(collapsed);
    // Show less takes it back to the first few; the chevron closes the range.
    await dialog.$("button=Show less").click();
    await expect(dialog.$$('li[data-kind="pattern-link"]')).toBeElementsArrayOfSize(3);
    await dialog.$('button[aria-label^="Hide set01.bin"]').click();
    await expect(dialog.$$('li[data-kind="pattern-link"]')).toBeElementsArrayOfSize(0);
    await dialog.$("button=Cancel").click();
    await waitForGone(ADD_DIALOG);
  });

  it("adds a named batch into its folder as one group row with a rollup", async () => {
    // A failed download of its own, to show Retry failed leaves it alone.
    const before = new Set((await engineRows()).map((d) => d.id));
    const own = await openAddDialog(root);
    await own.$(LINKS).setValue(`${baseUrl}/missing/loose.bin`);
    await own.$("button=Add download").click();
    await waitForGone(ADD_DIALOG);
    loose = (await newIds(before, 1))[0];
    ids.push(loose);
    await waitForStatus(loose, "failed", 30_000);

    const known = new Set((await engineRows()).map((d) => d.id));
    await paste([
      `${baseUrl}/any/one.bin`,
      `${baseUrl}/any/two.bin`,
      `${baseUrl}/missing/three.bin`,
    ]);
    const dialog = $(ADD_DIALOG);
    await setDestination(root);
    await dialog.$(BATCH_NAME).setValue("E2E set");
    await expect(dialog.$(FOLDER_TICK)).toBeSelected();
    await dialog.$("button=Add 3 downloads").click();
    await waitForGone(ADD_DIALOG);

    items = await newIds(known, 3);
    ids.push(...items);
    const rows = (await engineRows()).filter((d) => items.includes(d.id));
    batch = rows[0].batch!;
    expect(batch).toBe(items[0]);
    expect(rows.every((d) => d.batch === batch && d.batch_label === "E2E set")).toBe(true);
    for (const id of items) {
      await browser.waitUntil(
        async () => ["completed", "failed"].includes((await attr(id, "data-status")) ?? ""),
        {
          timeout: 30_000,
          timeoutMsg: `download ${id} never finished`,
        },
      );
    }

    const done = (await engineRows()).filter((d) => items.includes(d.id) && d.final_path);
    expect(done).toHaveLength(2);
    for (const d of done) {
      expect(path.dirname(d.final_path!)).toBe(path.join(root, "E2E set"));
    }

    const row = `${BATCH_ROW}[data-batch="${batch}"]`;
    await waitForText(`${row} [data-testid="batch-name"]`, "E2E set");
    await waitForText(`${row} [data-testid="batch-rollup"]`, "2 of 3 done");
    await waitForText(`${row} [data-testid="batch-failed"]`, "1 failed");
    await expect($(`${row}`)).toHaveAttribute("data-count", "3");
    // Show in folder opens the batch's folder.
    await $(`${row} [data-testid="batch-reveal"]`).click();
    const opened = path.join(root, "E2E set");
    await browser.waitUntil(
      () => readFileSync(process.env.RDM_OPEN_LOG!, "utf8").includes(`reveal ${opened}`),
      {
        timeoutMsg: "the batch's folder never reached the open log",
      },
    );
    // The group row sits above its files, which follow it directly.
    const after = await browser.execute((id) => {
      const all = Array.from(document.querySelectorAll("tbody tr"));
      const at = all.findIndex(
        (r) =>
          r.getAttribute("data-batch") === String(id) &&
          r.getAttribute("data-testid") === "batch-row",
      );
      return all
        .slice(at + 1, at + 4)
        .map((r) => `${r.getAttribute("data-testid")}:${r.getAttribute("data-batch")}`);
    }, batch);
    expect(after.filter((r) => r.startsWith("download-row:")).length).toBeGreaterThanOrEqual(1);
    expect(
      after.every((r) => r === `download-row:${batch}` || r.startsWith("download-error")),
    ).toBe(true);
  });

  it("marks a link already in the list with an info mark, and still adds it", async () => {
    await paste([`${baseUrl}/any/one.bin`, `${baseUrl}/file`]);
    const dialog = $(ADD_DIALOG);
    await expect(dialog.$('li[data-kind="link"][data-dup="listed"]')).toBeExisting();
    await expect(dialog.$$('li[data-kind="link"]')).toBeElementsArrayOfSize(2);
    await expect(dialog.$("button=Add 2 downloads")).toBeEnabled();
    await dialog.$("button=Cancel").click();
    await waitForGone(ADD_DIALOG);
  });

  it("retries a batch's failed files only", async () => {
    const [failedItem] = (await engineRows()).filter(
      (d) => items.includes(d.id) && d.status === "failed",
    );
    const before = [
      await attr(failedItem.id, "data-finished-at"),
      await attr(loose, "data-finished-at"),
    ];
    expect(before.every((t) => t !== null && t !== "")).toBe(true);
    // Finish times are in seconds: make sure a re-failure lands in a later one.
    await browser.pause(1100);

    await $(`${BATCH_ROW}[data-batch="${batch}"] [data-testid="batch-retry"]`).click();
    await browser.waitUntil(
      async () => {
        const at = await attr(failedItem.id, "data-finished-at");
        return (
          (await attr(failedItem.id, "data-status")) === "failed" &&
          at !== null &&
          at !== "" &&
          at !== before[0]
        );
      },
      { timeout: 30_000, timeoutMsg: "Retry failed didn't restart the batch's failed file" },
    );
    expect(await attr(loose, "data-finished-at")).toBe(before[1]);
    expect((await engineRow(loose))?.status).toBe("failed");
  });

  it("closes and opens a batch's files from its group row", async () => {
    const hide = `${BATCH_ROW}[data-batch="${batch}"] button[aria-label="Hide E2E set"]`;
    await $(hide).click();
    await waitForGone(`tr[data-testid="download-row"][data-batch="${batch}"]`);
    await $(`${BATCH_ROW}[data-batch="${batch}"] button[aria-label="Show E2E set"]`).click();
    await expect(
      $$(`tr[data-testid="download-row"][data-batch="${batch}"]`),
    ).toBeElementsArrayOfSize(3);
  });

  it("closes the list with an Ungrouped header over downloads in no batch", async () => {
    const header = 'tr[data-testid="ungrouped-row"]';
    const looseRow = `tr[data-testid="download-row"][data-id="${loose}"]`;
    await $(header).waitForExist();
    const order = await browser.execute(
      (h, l) => {
        const all = Array.from(document.querySelectorAll("tbody tr"));
        const at = all.findIndex((r) => r.matches(h));
        return {
          last: all.slice(at + 1).every((r) => !r.matches('[data-testid="batch-row"]')),
          looseAfter: all.findIndex((r) => r.matches(l)) > at,
          count: all.slice(at + 1).filter((r) => r.matches('[data-testid="download-row"]')).length,
        };
      },
      header,
      looseRow,
    );
    expect(order.last && order.looseAfter).toBe(true);
    await expect($(header)).toHaveAttribute("data-count", String(order.count));
    // No bar, retry or folder button on it.
    await expect($(`${header} [role="progressbar"]`)).not.toBeExisting();
    await expect($(`${header} button`)).toHaveAttribute("aria-label", "Hide ungrouped downloads");

    await $(`${header} button`).click();
    await waitForGone(looseRow);
    await $(`${header} button`).click();
    await $(looseRow).waitForExist();
  });

  it("turns grouping off for a flat list, and remembers it", async () => {
    const toggle = $(GROUP_TOGGLE);
    await expect(toggle).toHaveAttribute("aria-pressed", "true");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-pressed", "false");
    await waitForGone(BATCH_ROW);
    await waitForGone('tr[data-testid="ungrouped-row"]');
    for (const id of items)
      await $(`tr[data-testid="download-row"][data-id="${id}"]`).waitForExist();
    const saved = await browser.execute((key) => localStorage.getItem(key), PREFS_KEY);
    expect(JSON.parse(saved!).groupBatches).toBe(false);

    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-pressed", "true");
    await $(`${BATCH_ROW}[data-batch="${batch}"]`).waitForExist();
  });

  it("filters to one batch from the sidebar, apart from tags", async () => {
    const button = $(`button[data-testid="batch-filter"][data-batch="${batch}"]`);
    await expect(button).toHaveAttribute("data-count", "3");
    expect(await listedTags()).not.toContain("E2E set");
    await button.click();
    await expect(button).toHaveAttribute("aria-current", "true");

    await browser.waitUntil(async () => {
      const shown = await shownIds();
      return shown.length === 3 && items.every((id) => shown.includes(id));
    });
    // The batch's own view is the group already: its rollup is in the toolbar, with no group row.
    await waitForGone(BATCH_ROW);
    await waitForText('[data-testid="batch-view-rollup"]', "2 of 3 done");
    await expect($('[data-testid="view-count"]')).toHaveText("3");
    await selectFilter("all");
  });
});
