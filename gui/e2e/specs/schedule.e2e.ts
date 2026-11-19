// End-to-end: the queue window. A rule made in the Schedules view's New schedule form (its
// window set to a stretch that is closed now) pauses the queue: the status bar says so once,
// and the Schedules view lists the rule and says when the window opens. Switching the rule off
// clears the note and switching it on brings it back. A second rule is made with Choose days
// (an empty set of days is refused in the form), then Delete removes it. A Speed limit rule is
// made through the same form (Type switched, Limit typed): it is listed after the windows and
// the status line says when its cap starts.
// The rules live in the shared app state, so the spec empties them before and after.

import { $, $$, browser, expect } from "@wdio/globals";
import { clock, invoke, retype, selectFilter, waitForGone, waitForText } from "../helpers";

const SCHEDULES_LINK = '[data-testid="schedules-link"]';
const VIEW = '[data-testid="schedules-view"]';
const FORM = '[role="dialog"][aria-label="New schedule"]';
const PAUSED = '[data-testid="queue-paused"]';
const STATUS = '[data-testid="schedule-status"]';
const SUMMARY = '[data-testid="schedule-summary"]';
const RULE = '[data-testid="rule-row"]';

interface SavedSchedule {
  queue_windows: { name?: string; enabled: boolean; window: { days: string[] } }[];
  speed_entries: { name?: string; enabled: boolean; limit: number; window: { days: string[] } }[];
}

const getSchedule = () => invoke<SavedSchedule>("get_schedule");
const clearRules = () =>
  invoke("set_schedule", { schedule: { queue_windows: [], speed_entries: [] } });

/** A window that is closed now, by the page's own local clock: two to three hours ahead. */
async function closedWindow(): Promise<{ start: string; end: string }> {
  const hours = await browser.execute(() => new Date().getHours());
  const at = (h: number) => `${String(h % 24).padStart(2, "0")}:00`;
  return { start: at(hours + 2), end: at(hours + 3) };
}

const ruleCount = async () => (await $$(RULE)).length;

async function waitForRules(n: number): Promise<void> {
  await browser.waitUntil(async () => (await ruleCount()) === n, {
    timeoutMsg: `the Schedules view never listed ${n} rule(s)`,
  });
}

/** Opens the New schedule form and types the times (not Enter: the form stays open). */
async function openForm(win: { start: string; end: string }): Promise<void> {
  await $('[data-testid="new-schedule"]').click();
  await $(FORM).waitForDisplayed();
  await retype($('input[aria-label="Start time"]'), win.start, { enter: false });
  await retype($('input[aria-label="End time"]'), win.end, { enter: false });
}

describe("rdm GUI queue window", () => {
  let win: { start: string; end: string };

  before(async () => {
    await clearRules();
    win = await closedWindow();
    await $(SCHEDULES_LINK).click();
    await $(VIEW).waitForDisplayed();
  });

  after(async () => {
    await clearRules();
    await selectFilter("all");
  });

  it("starts with no rules and an open queue", async () => {
    await waitForText(STATUS, "No schedules");
    await expect($(PAUSED)).not.toBeDisplayed();
  });

  it("says what a new window does before it is added", async () => {
    await openForm(win);
    await $('input[aria-label="Name"]').setValue("E2E window");
    const [from, to] = [await clock(win.start), await clock(win.end)];
    await waitForText(
      SUMMARY,
      `Downloads run from ${from} to ${to} every day, and pause at ${to}.`,
    );
    await $('[data-testid="finish-running"]').click();
    await waitForText(
      SUMMARY,
      `New downloads start from ${from} to ${to} every day. Running ones finish.`,
    );
    await $('[data-testid="finish-running"]').click();
  });

  it("adds the rule, lists it and pauses the queue once in the status bar", async () => {
    await $('[data-testid="schedule-submit"]').click();
    await waitForGone(FORM);
    await waitForRules(1);
    await waitForText('[data-testid="rule-name"]', "E2E window");
    await waitForText(
      '[data-testid="rule-summary"]',
      `from ${await clock(win.start)} to ${await clock(win.end)}`,
    );

    await waitForText(PAUSED, "Queue paused until");
    await waitForText(PAUSED, await clock(win.start));
    expect(await $$(PAUSED).length).toBe(1);
    await waitForText(STATUS, "Queue paused. The window opens at");

    const saved = await getSchedule();
    expect(saved.queue_windows).toHaveLength(1);
    expect(saved.queue_windows[0].window.days).toHaveLength(7);
  });

  it("switches the rule off and on", async () => {
    const toggle = $('[data-testid="rule-switch"]');
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "false");
    await waitForGone(PAUSED);
    await waitForText(STATUS, "All schedules are off");
    await expect($(RULE)).toHaveAttribute("data-enabled", "false");
    expect((await getSchedule()).queue_windows[0].enabled).toBe(false);

    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "true");
    await waitForText(PAUSED, "Queue paused until");
    await expect($(RULE)).toHaveAttribute("data-enabled", "true");
  });

  it("makes a second rule with chosen days and refuses an empty set", async () => {
    await openForm(win);
    await $('select[aria-label="Days"]').selectByAttribute("value", "choose");
    const chip = (day: string) => $(`[data-testid="day-chips"] button[data-day="${day}"]`);
    // All seven start picked; leave Mon and Tue.
    for (const day of ["wed", "thu", "fri", "sat", "sun"]) await chip(day).click();
    await waitForText(SUMMARY, "on Mon and Tue");

    await chip("mon").click();
    await chip("tue").click();
    await waitForText(SUMMARY, "Pick at least one day");
    await expect($('[data-testid="schedule-submit"]')).toBeDisabled();

    await chip("mon").click();
    await chip("tue").click();
    await waitForText(SUMMARY, "on Mon and Tue");
    await $('[data-testid="schedule-submit"]').click();
    await waitForGone(FORM);
    await waitForRules(2);

    const saved = await getSchedule();
    expect(saved.queue_windows[1].window.days).toEqual(["mon", "tue"]);
    await waitForText(
      `${RULE}:nth-child(2) [data-testid="rule-name"]`,
      `${await clock(win.start)} to ${await clock(win.end)}, Mon and Tue`,
    );
  });

  it("deletes a rule from its menu", async () => {
    await $(`${RULE}:nth-child(2) [data-testid="rule-menu-button"]`).click();
    await $('[data-testid="rule-menu"]').$("button=Delete").click();
    await waitForRules(1);
    expect((await getSchedule()).queue_windows).toHaveLength(1);
    await waitForText(PAUSED, "Queue paused until");
  });

  it("makes a Speed limit rule through the form, lists it and says when its cap starts", async () => {
    await $('[data-testid="new-schedule"]').click();
    await $(FORM).waitForDisplayed();
    await $('select[aria-label="Type"]').selectByAttribute("value", "speed");
    await expect($('[data-testid="finish-running"]')).not.toBeDisplayed();
    await $('input[aria-label="Limit"]').setValue("200");
    // The unit offers KB/s as well as MB/s.
    await expect($('select[aria-label="Unit"] option[value="K"]')).toHaveText("KB/s");
    await $('select[aria-label="Unit"]').selectByAttribute("value", "K");
    // A new Speed limit starts on weekdays; every day keeps the cap's start independent of today.
    await $('select[aria-label="Days"]').selectByAttribute("value", "daily");
    await retype($('input[aria-label="Start time"]'), win.start, { enter: false });
    await retype($('input[aria-label="End time"]'), win.end, { enter: false });
    await waitForText(
      SUMMARY,
      `Downloads are limited to 200 KB/s from ${await clock(win.start)} to ${await clock(win.end)} every day.`,
    );
    await $('input[aria-label="Name"]').setValue("E2E cap");
    await $('[data-testid="schedule-submit"]').click();
    await waitForGone(FORM);
    await waitForRules(2);

    await waitForText(`${RULE}:nth-child(2) [data-testid="rule-name"]`, "E2E cap");
    await waitForText(
      `${RULE}:nth-child(2) [data-testid="rule-summary"]`,
      `200 KB/s, ${await clock(win.start)}–${await clock(win.end)}, daily`,
    );
    await waitForText(STATUS, `A 200 KB/s cap starts at ${await clock(win.start)}.`);

    const saved = await getSchedule();
    expect(saved.speed_entries).toHaveLength(1);
    expect(saved.speed_entries[0].limit).toBe(200 * 1024);
    expect(saved.queue_windows).toHaveLength(1);
  });

  it("switches the speed rule off and the cap leaves the status line", async () => {
    const toggle = $(`${RULE}:nth-child(2) [data-testid="rule-switch"]`);
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "false");
    await browser.waitUntil(async () => !(await $(STATUS).getText()).includes("cap"), {
      timeoutMsg: "the status line still mentioned the switched-off cap",
    });
    expect((await getSchedule()).speed_entries[0].enabled).toBe(false);
  });

  it("takes a time typed loosely and tidies it to HH:MM", async () => {
    await $('[data-testid="new-schedule"]').click();
    await $(FORM).waitForDisplayed();
    const start = $('input[aria-label="Start time"]');
    await retype(start, "930", { enter: false });
    await waitForText(SUMMARY, `from ${await clock("09:30")} to`);
    // Leaving the box rewrites it as HH:MM.
    await $('input[aria-label="Name"]').click();
    await expect(start).toHaveValue("09:30");
    await $('[role="dialog"] button=Cancel').click();
    await waitForGone(FORM);
  });
});
