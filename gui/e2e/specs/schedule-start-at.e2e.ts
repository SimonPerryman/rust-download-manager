// End-to-end: a start time. A link added with a Start at (More options in the Add dialog) is held
// out of the queue and its row says "Scheduled for HH:MM"; the Schedules view lists it under
// Coming up ("Starts today HH:MM", an Edit pencil and a Start now button), and Start now sends it
// into the queue, where it finishes. A row's context menu has Start at…, which moves a schedule.
// The start time is two hours ahead by the page's own clock, so it never comes due mid-spec.

import { $, $$, browser, expect } from "@wdio/globals";
import {
  LINKS,
  ROW_MENU,
  baseUrl,
  engineRows,
  invoke,
  openAddDialog,
  openRowMenu,
  removeAll,
  rowById,
  selectFilter,
  waitForGone,
  waitForStatus,
  waitForText,
  clock,
  clockWhen,
  newIds,
  ADD_DIALOG,
} from "../helpers";

const SCHEDULES_LINK = '[data-testid="schedules-link"]';
const COMING_UP = '[data-testid="coming-up"]';
const GROUP = '[data-testid="start-group"]';
const START_AT_DIALOG = '[data-testid="start-at-dialog"]';

interface StartRow {
  id: number;
  status: string;
  start_at: string | null;
  start_group: number | null;
  start_missed: boolean;
}
const startRow = async (id: number) => (await invoke<StartRow[]>("list")).find((d) => d.id === id)!;

/** `HH:00`, two hours ahead by the page's local clock, with the hours kept to a day. */
async function hoursAhead(n: number): Promise<string> {
  const hours = await browser.execute(() => new Date().getHours());
  return `${String((hours + n) % 24).padStart(2, "0")}:00`;
}

describe("rdm GUI start time", () => {
  const ids: number[] = [];
  let time: string;

  before(async () => {
    time = await hoursAhead(2);
  });

  after(async () => {
    await removeAll(ids);
    await selectFilter("all");
  });

  it("holds a download added with a Start at, and its row says when", async () => {
    const before = new Set((await engineRows()).map((d) => d.id));
    const dialog = await openAddDialog();
    await dialog.$(LINKS).setValue(`${baseUrl}/file?start-at`);
    await dialog.$("button=More options").click();
    // Typed without the colon, as people do: 1400 means 14:00.
    await dialog.$('input[aria-label="Start at"]').setValue(time.replace(":", ""));
    await waitForText('[data-testid="add-start-at-echo"]', await clock(time));
    await dialog.$("button=Add download").waitForEnabled();
    await dialog.$("button=Add download").click();
    await waitForGone(ADD_DIALOG);

    ids.push(...(await newIds(before, 1)));
    const id = ids[0];
    await rowById(id).waitForExist();
    await waitForText(`tr[data-id="${id}"]`, `Scheduled for ${await clockWhen(time)}`);

    // Held for two hours: a link this small would be done by now if it were in the queue.
    await browser.pause(1500);
    const held = await startRow(id);
    expect(held.status).toBe("queued");
    expect(held.start_at).toMatch(new RegExp(`T${time}$`));
    expect(held.start_missed).toBe(false);
  });

  it("lists it under Coming up with Edit and Start now", async () => {
    await $(SCHEDULES_LINK).click();
    await $(COMING_UP).waitForDisplayed();
    expect(await $$(`${COMING_UP} ${GROUP}`).length).toBe(1);
    await waitForText(`${GROUP} [data-testid="start-group-when"]`, await clock(time));
    await waitForText(`${GROUP} [data-testid="start-group-when"]`, "Starts");
    // The pencil is icon-only and says what it is in a tooltip.
    await expect($(`${GROUP} [data-testid="start-group-edit"]`)).toHaveAttribute("title", "Edit");
    await expect($('[data-testid="missed-notice"]')).not.toBeDisplayed();
    await expect($('[data-testid="schedules-missed"]')).not.toBeDisplayed();
  });

  it("starts it at once with Start now, and it finishes", async () => {
    await $(`${GROUP} [data-testid="start-group-now"]`).click();
    await waitForGone(COMING_UP);
    await selectFilter("all");
    await waitForStatus(ids[0], "completed");
    expect((await startRow(ids[0])).start_at).toBeNull();
  });

  it("moves a schedule with Start at… from a row's menu", async () => {
    const before = new Set((await engineRows()).map((d) => d.id));
    // Added with a time through the backend, then given another one from the row.
    await invoke("add_download", {
      url: `${baseUrl}/file?second`,
      dir: null,
      checksum: null,
      limit: null,
      tags: null,
      startAt: `${time}`,
    });
    const [second] = await newIds(before, 1);
    ids.push(second);
    await selectFilter("all");
    await rowById(second).waitForExist();

    const later = await hoursAhead(3);
    await openRowMenu(second);
    await $(ROW_MENU).$("button=Start at…").click();
    await $(START_AT_DIALOG).waitForDisplayed();
    await $(`${START_AT_DIALOG} input[aria-label="Start at"]`).setValue(later);
    await waitForText('[data-testid="start-at-echo"]', await clock(later));
    await $('[data-testid="start-at-submit"]').click();
    await waitForGone(START_AT_DIALOG);

    await browser.waitUntil(
      async () => (await startRow(second)).start_at?.endsWith(later) ?? false,
      {
        timeoutMsg: "the row's Start at… never moved its schedule",
      },
    );
    await waitForText(`tr[data-id="${second}"]`, await clock(later));
    expect((await startRow(second)).status).toBe("queued");
  });
});
