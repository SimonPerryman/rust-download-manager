// End-to-end: tags. A tag typed in the Add dialog shows on the row and under
// Tags with its count; Tags… from the row menu adds more. A tag filter shows
// exactly its rows and its count drops as one is archived. The Tags section
// follows the view: under Archived it lists only archived rows' tags, and a
// tag there narrows the Archived view.

import { $, browser, expect } from "@wdio/globals";
import {
  ADD_DIALOG,
  LINKS,
  ROW_MENU,
  addThroughDialog,
  baseUrl,
  clickRowAction,
  engineRows,
  listedTags,
  newIds,
  openAddDialog,
  openRowMenu,
  removeAll,
  rowById,
  rowSelector,
  selectFilter,
  selectTag,
  shownIds,
  tagCount,
  waitForGone,
  waitForStatus,
  waitForText,
} from "../helpers";

const TAGS_DIALOG = '[data-testid="tags-dialog"]';

/** Waits until the `tag` filter's count reads `n` (null: not listed). */
async function waitForTagCount(tag: string, n: number | null): Promise<void> {
  await browser.waitUntil(async () => (await tagCount(tag)) === n, {
    timeoutMsg: `the ${tag} tag's count never read ${String(n)}`,
  });
}

/** Waits until the view shows exactly `ids` (any order). */
async function waitForShown(ids: number[], what: string): Promise<void> {
  const want = [...ids].sort((a, b) => a - b);
  await browser.waitUntil(
    async () => JSON.stringify((await shownIds()).sort((a, b) => a - b)) === JSON.stringify(want),
    { timeoutMsg: `${what} didn't show exactly downloads ${want.join(", ")}` },
  );
}

/** Adds tags to `id` through the row menu's Tags… dialog. */
async function tagThroughMenu(id: number, text: string): Promise<void> {
  await openRowMenu(id);
  await $(ROW_MENU).$('button[data-cmd="tags"]').click();
  const dialog = $(TAGS_DIALOG);
  await dialog.waitForDisplayed();
  await dialog.$('input[aria-label="Add tags"]').setValue(text);
  await dialog.$("button=Add").click();
  for (const tag of text.split(",").map((t) => t.trim())) {
    await dialog
      .$(`[data-testid="tag-chip"][data-tag="${tag}"]`)
      .waitForExist({ timeoutMsg: `the Tags dialog never listed ${tag}` });
  }
  await dialog.$("button=Done").click();
  await waitForGone(TAGS_DIALOG);
}

describe("rdm GUI tags", () => {
  const ours: number[] = [];
  let tagged = 0;
  let second = 0;
  let plain = 0;

  before(async () => {
    await selectFilter("all");
  });

  after(async () => {
    await removeAll(ours);
  });

  it("shows a tag given at add on the row and under Tags", async () => {
    const before = new Set((await engineRows()).map((d) => d.id));
    const dialog = await openAddDialog();
    await dialog.$(LINKS).setValue(`${baseUrl}/file`);
    await dialog.$('input[aria-label="Tags"]').setValue("work");
    await dialog.$("button=Add download").click();
    await waitForGone(ADD_DIALOG);
    [tagged] = await newIds(before, 1);
    ours.push(tagged);
    await rowById(tagged).waitForExist();
    await waitForStatus(tagged, "completed", 30_000);

    await waitForText(`${rowSelector(tagged)} [data-testid="row-tag"]`, "work");
    expect((await engineRows()).find((d) => d.id === tagged)?.tags).toEqual(["work"]);
    await waitForTagCount("work", 1);
  });

  it("filters by tag with matching counts, and archiving drops the count", async () => {
    second = await addThroughDialog(`${baseUrl}/file`);
    ours.push(second);
    plain = await addThroughDialog(`${baseUrl}/file`);
    ours.push(plain);
    await waitForStatus(second, "completed", 30_000);
    await waitForStatus(plain, "completed", 30_000);
    await tagThroughMenu(second, "work, fresh");
    await waitForTagCount("work", 2);

    await selectTag("work");
    await waitForText("h1", "work");
    await waitForShown([tagged, second], "the work filter");
    await expect($('[data-testid="view-count"]')).toHaveText("2");

    await clickRowAction(tagged, "Archive");
    await waitForTagCount("work", 1);
    await waitForShown([second], "the work filter after archiving one");
  });

  it("lists only archived tags under Archived, and a tag narrows it", async () => {
    await selectFilter("all");
    await clickRowAction(plain, "Archive");
    await waitForGone(rowSelector(plain));

    await selectFilter("archived");
    await waitForShown([tagged, plain], "Archived");
    await browser.waitUntil(
      async () => JSON.stringify(await listedTags()) === JSON.stringify(["work"]),
      { timeoutMsg: "the Tags section under Archived didn't list just the archived tag" },
    );
    await waitForTagCount("work", 1);

    await selectTag("work");
    await waitForText("h1", "Archived");
    await waitForText("h1", "work");
    await waitForShown([tagged], "Archived › work");

    // Back out of the archive, the Tags section lists the working rows' tags again.
    await selectFilter("all");
    await browser.waitUntil(
      async () => {
        const tags = await listedTags();
        return tags.includes("fresh") && tags.includes("work");
      },
      { timeoutMsg: "the Tags section never went back to the working rows' tags" },
    );
    await waitForTagCount("work", 1);
  });
});
