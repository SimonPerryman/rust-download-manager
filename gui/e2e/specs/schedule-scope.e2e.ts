// End-to-end: a rule scoped to a tag. Applies to is set to A tag in the New schedule form, the
// summary names the tag, and the saved rule carries the scope and is listed with it. A scoped
// window leaves the queue open (the status bar and status line speak for all downloads), and a
// download it holds says "Queued until HH:MM" on its row while an untagged one runs.

import { $, $$, browser, expect } from "@wdio/globals";
import {
  baseUrl,
  clock,
  clockWhen,
  engineRows,
  invoke,
  newIds,
  rectOf,
  removeAll,
  rowById,
  selectFilter,
  waitForGone,
  waitForStatus,
  waitForText,
} from "../helpers";

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
  const ids: number[] = [];

  before(async () => {
    await clearRules();
    await $(SCHEDULES_LINK).click();
    await $(VIEW).waitForDisplayed();
  });

  after(async () => {
    await clearRules();
    await removeAll(ids);
    await selectFilter("all");
  });

  it("makes a rule for a tag through the form and lists it with its scope", async () => {
    const open = await clock("22:00");
    const close = await clock("08:00");
    await $('[data-testid="new-schedule"]').click();
    await $(FORM).waitForDisplayed();
    // Empty rows start collapsed; typing a tag nothing carries is accepted with a note.
    await $('[data-testid="scope-add-tag"]').click();
    const tags = $('input[aria-label="Tags"]');
    await tags.addValue("nobody-1");
    await tags.addValue("");
    await tags.addValue("nobody-2");
    await tags.addValue("");
    await $('[data-testid="scope-add-host"]').click();
    const hosts = $('input[aria-label="Hosts"]');
    await hosts.addValue("cdn.example.com");
    await hosts.addValue("");
    await $('input[aria-label="Name"]').setValue("E2E tagged");
    await waitForText(`${FORM} [data-testid="scope-tag"]`, "No downloads have the tag nobody-1");
    await waitForText(
      SUMMARY,
      `Downloads tagged nobody-1 or nobody-2, from cdn.example.com run from ${open} to ${close} every day, and pause at ${close}.`,
    );
    await $('[data-testid="schedule-submit"]').click();
    await waitForGone(FORM);
    await browser.waitUntil(async () => (await $$(RULE)).length === 1);

    await waitForText(
      `${RULE} [data-testid="rule-summary"]`,
      `Downloads tagged nobody-1 or nobody-2, from cdn.example.com run from ${open} to ${close} every day, and pause at ${close}.`,
    );
    await expect($(`${RULE} [data-testid="rule-warning-icon"]`)).toBeDisplayed();
    await waitForText(`${RULE}`, "No downloads have the tag nobody-1");
    // The names sit in chips, and the row's sentence stays inside the row.
    const chip = await rectOf(`${RULE} [data-testid="scope-name"]`);
    const row = await rectOf(RULE);
    expect(chip.y).toBeGreaterThanOrEqual(row.y);
    expect(chip.y + chip.height).toBeLessThanOrEqual(row.y + row.height);
    await waitForText(STATUS, "except those a rule names");
    await expect($(PAUSED)).not.toBeDisplayed();

    const saved = await invoke<Saved>("get_schedule");
    expect(saved.queue_windows[0].scope).toEqual({
      tags: ["nobody-1", "nobody-2"],
      hosts: ["cdn.example.com"],
    });
  });

  it("loads a rule saved with the old single scope as a list of one", async () => {
    await invoke("set_schedule", {
      schedule: {
        queue_windows: [
          {
            name: "Old tag",
            enabled: true,
            window: {
              start: "22:00",
              end: "08:00",
              days: ["mon", "tue", "wed", "thu", "fri", "sat", "sun"],
            },
            finish_running: false,
            scope: { kind: "tag", tag: "legacy" },
          },
        ],
        speed_entries: [],
      },
    });
    const saved = await invoke<Saved>("get_schedule");
    expect(saved.queue_windows[0].scope).toEqual({ tags: ["legacy"] });
    await clearRules();
  });

  it("says Queued until the window opens on a tagged download, and only on that one", async () => {
    const hours = await browser.execute(() => new Date().getHours());
    const at = (h: number) => `${String(h % 24).padStart(2, "0")}:00`;
    const days = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
    await invoke("set_schedule", {
      schedule: {
        queue_windows: [
          {
            enabled: true,
            window: { start: at(hours + 2), end: at(hours + 3), days },
            finish_running: false,
            scope: { kind: "tag", tag: "e2e-held" },
          },
        ],
        speed_entries: [],
      },
    });

    const before = new Set((await engineRows()).map((d) => d.id));
    const add = (name: string, tags: string[] | null) =>
      invoke("add_download", {
        url: `${baseUrl}/file?${name}`,
        dir: null,
        checksum: null,
        limit: null,
        tags,
        startAt: null,
      });
    await add("held", ["e2e-held"]);
    await add("free", null);
    const [first, second] = await newIds(before, 2);
    ids.push(first, second);
    const rows = await engineRows();
    const held = rows.find((d) => d.id === first)!.url.endsWith("held") ? first : second;
    const free = held === first ? second : first;

    await selectFilter("all");
    await rowById(held).waitForExist();
    await waitForText(`tr[data-id="${held}"]`, `Queued until ${await clockWhen(at(hours + 2))}`);
    await expect(rowById(held)).not.toHaveAttribute("data-next-up");
    await waitForStatus(free, "completed");
    const freeText = await browser.execute(
      (id) => (document.querySelector(`tr[data-id="${id}"]`) as HTMLElement).innerText,
      free,
    );
    expect(freeText).not.toContain("Queued until");
    expect((await engineRows()).find((d) => d.id === held)!.status).toBe("queued");
  });
});
