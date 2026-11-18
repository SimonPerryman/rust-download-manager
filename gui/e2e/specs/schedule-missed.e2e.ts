// End-to-end: start times that passed while rdm was closed. The config seeds four of them before
// this spec's app starts (see seedMissed.ts): a batch "Season 2" of two files and three single
// downloads. None starts by itself. The notice is on the main list at launch and says "4 scheduled
// starts were missed (5 downloads)", the sidebar counts schedules (4), and each row says "Was due".
// In the Schedules view each missed schedule has its own Reschedule menu and Start now; Dismiss
// on the notice leaves the downloads paused in the list.

import { $, $$, browser, expect } from "@wdio/globals";
import { engineRows, invoke, removeAll, selectFilter, waitForGone, waitForText } from "../helpers";

const NOTICE = '[data-testid="missed-notice"]';
const NOTICE_TEXT = '[data-testid="missed-text"]';
const SIDEBAR_COUNT = '[data-testid="schedules-missed"]';
const SCHEDULES_LINK = '[data-testid="schedules-link"]';
const MISSED_LIST = '[data-testid="missed-list"]';
const COMING_UP = '[data-testid="coming-up"]';
const GROUP = '[data-testid="start-group"]';

interface StartRow {
  id: number;
  filename: string;
  status: string;
  start_at: string | null;
  start_missed: boolean;
}
const rows = async () =>
  (await invoke<StartRow[]>("list")).filter((d) => /season-2|missed-/.test(d.filename));
const byName = async (name: string) => (await rows()).find((d) => d.filename.startsWith(name))!;

describe("rdm GUI missed start times", () => {
  after(async () => {
    await removeAll((await rows()).map((d) => d.id));
    await selectFilter("all");
  });

  it("shows the notice on the main list at launch, and starts nothing", async () => {
    await waitForText(NOTICE_TEXT, "4 scheduled starts were missed (5 downloads)");
    await waitForText(SIDEBAR_COUNT, "4");
    await expect($(SIDEBAR_COUNT)).toHaveAttribute("aria-label", "4 missed");

    // Left alone for a few clock ticks: a missed start waits for the user.
    await browser.pause(1500);
    const all = await rows();
    expect(all).toHaveLength(5);
    for (const d of all) {
      expect(d.status).toBe("queued");
      expect(d.start_missed).toBe(true);
      expect(d.start_at).toBe("2020-01-06T22:00");
    }
    await waitForText("table", "Was due 6 Jan 22:00");
  });

  it("lists them under Missed in the Schedules view, with the notice above", async () => {
    await $(SCHEDULES_LINK).click();
    await $(MISSED_LIST).waitForDisplayed();
    expect(await $$(`${MISSED_LIST} ${GROUP}`).length).toBe(4);
    await expect($(NOTICE)).toBeDisplayed();
    await waitForText(`${MISSED_LIST} ${GROUP}:first-child`, "Season 2");
    await waitForText(`${MISSED_LIST} ${GROUP}:first-child`, "2 files");
    await waitForText(`${MISSED_LIST} ${GROUP}:first-child`, "Was due 6 Jan 22:00");
  });

  it("reschedules one from its menu to a real time, and it moves to Coming up", async () => {
    const batch = `${MISSED_LIST} ${GROUP}:first-child`;
    await $(`${batch} [data-testid="reschedule-button"]`).click();
    await $('[data-testid="reschedule-menu"]').waitForDisplayed();
    const choices = await browser.execute(() =>
      Array.from(document.querySelectorAll('[data-testid="reschedule-choice"]')).map((e) => ({
        text: (e as HTMLElement).innerText,
        at: e.getAttribute("data-at")!,
      })),
    );
    // Tonight is offered only before 22:00; Tomorrow and Next week always are.
    const named = (name: string) => choices.find((c) => c.text.includes(name));
    expect(named("Next week")).toBeDefined();
    expect(choices.every((c) => /\d\d:\d\d/.test(c.text))).toBe(true);
    const at = named("Tomorrow")!.at;
    const tomorrow = $(`[data-testid="reschedule-choice"][data-at="${at}"]`);
    await tomorrow.click();

    await $(COMING_UP).waitForDisplayed();
    await waitForText(`${COMING_UP} ${GROUP}`, "Season 2");
    await waitForText(`${COMING_UP} ${GROUP}`, "Starts");
    await waitForText(NOTICE_TEXT, "3 scheduled starts were missed (3 downloads)");
    const season = await byName("season-2-a");
    expect(season.start_at).toBe(at);
    expect(season.start_missed).toBe(false);
    expect(season.status).toBe("queued");
  });

  it("starts one with Start now, and it finishes", async () => {
    await $(`${MISSED_LIST} ${GROUP}:first-child [data-testid="start-group-now"]`).click();
    await browser.waitUntil(async () => (await byName("missed-one")).status === "completed", {
      timeoutMsg: "Start now never ran the missed download",
    });
    await waitForText(NOTICE_TEXT, "2 scheduled starts were missed (2 downloads)");
  });

  it("is dealt with by Dismiss, which leaves the downloads paused in the list", async () => {
    await $('[data-testid="missed-dismiss"]').click();
    await waitForGone(NOTICE);
    await waitForGone(SIDEBAR_COUNT);
    await browser.waitUntil(
      async () =>
        (await byName("missed-two")).status === "paused" &&
        (await byName("missed-three")).status === "paused",
      { timeoutMsg: "Dismiss never left the downloads paused" },
    );
    expect((await byName("missed-two")).start_at).toBeNull();
    // Nothing was lost: they are all still in the engine's list.
    expect((await engineRows()).filter((d) => /missed-/.test(d.filename))).toHaveLength(3);
  });
});
