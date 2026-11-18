// End-to-end: a rule scoped to a tag. Applies to is set to A tag in the New schedule form, the
// summary names the tag, and the saved rule carries the scope and is listed with it. A scoped
// window leaves the queue open (the status bar and status line speak for all downloads).

import { $, $$, browser, expect } from "@wdio/globals";
import { invoke, waitForGone, waitForText } from "../helpers";

const SCHEDULES_LINK = '[data-testid="schedules-link"]';
const VIEW = '[data-testid="schedules-view"]';
const FORM = '[role="dialog"][aria-label="New schedule"]';
const SUMMARY = '[data-testid="schedule-summary"]';
const STATUS = '[data-testid="schedule-status"]';
const RULE = '[data-testid="rule-row"]';
const PAUSED = '[data-testid="queue-paused"]';

interface Saved {
  queue_windows: { name?: string; scope?: { kind: string; tag?: string } }[];
}

const clearRules = () =>
  invoke("set_schedule", { schedule: { queue_windows: [], speed_entries: [] } });

describe("rdm GUI scoped schedule rule", () => {
  before(async () => {
    await clearRules();
    await $(SCHEDULES_LINK).click();
    await $(VIEW).waitForDisplayed();
  });

  after(async () => {
    await clearRules();
  });

  it("makes a rule for a tag through the form and lists it with its scope", async () => {
    await $('[data-testid="new-schedule"]').click();
    await $(FORM).waitForDisplayed();
    await $('select[aria-label="Applies to"]').selectByAttribute("value", "tag");
    await $('input[aria-label="Tag"]').setValue("course-2");
    await $('input[aria-label="Name"]').setValue("E2E tagged");
    await waitForText(
      SUMMARY,
      "Downloads tagged course-2 run from 22:00 to 08:00 every day, and pause at 08:00.",
    );
    await $('[data-testid="schedule-submit"]').click();
    await waitForGone(FORM);
    await browser.waitUntil(async () => (await $$(RULE)).length === 1);

    await waitForText(
      `${RULE} [data-testid="rule-summary"]`,
      "Downloads tagged course-2 run from 22:00 to 08:00 every day, and pause at 08:00.",
    );
    await waitForText(STATUS, "except those a rule names");
    await expect($(PAUSED)).not.toBeDisplayed();

    const saved = await invoke<Saved>("get_schedule");
    expect(saved.queue_windows[0].scope).toEqual({ kind: "tag", tag: "course-2" });
  });
});
