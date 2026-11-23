// End-to-end: history. A download a closed window holds, and a tag added to it, show in its Log
// tab among the engine lines with a small type column; a rule's more menu opens its History
// dialog, which closes on Escape; and removing the download takes its events away.

import { $, $$, browser, expect } from "@wdio/globals";
import {
  baseUrl,
  dismissOverlays,
  engineRows,
  invoke,
  Key,
  newIds,
  removeAll,
  rowById,
  selectFilter,
  waitForGone,
  waitForText,
} from "../helpers";

const SCHEDULES_LINK = '[data-testid="schedules-link"]';
const VIEW = '[data-testid="schedules-view"]';
const PANE = '[data-testid="details-pane"]';
const LOG = `${PANE} [data-testid="log"]`;
const DIALOG = '[data-testid="rule-history-dialog"]';
const DAYS = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

interface Event {
  kind: string;
  details: string;
}

const clearRules = () =>
  invoke("set_schedule", { schedule: { queue_windows: [], speed_entries: [] } });

describe("rdm GUI history", () => {
  const ids: number[] = [];
  let held = 0;

  before(async () => {
    const hours = await browser.execute(() => new Date().getHours());
    const at = (h: number) => `${String(h % 24).padStart(2, "0")}:00`;
    await invoke("set_schedule", {
      schedule: {
        queue_windows: [
          {
            name: "E2E held",
            enabled: true,
            window: { start: at(hours + 2), end: at(hours + 3), days: DAYS },
            finish_running: false,
            scope: { tags: ["e2e-history"] },
          },
        ],
        speed_entries: [],
      },
    });
    const before = new Set((await engineRows()).map((d) => d.id));
    await invoke("add_download", {
      url: `${baseUrl}/file?history`,
      dir: null,
      checksum: null,
      limit: null,
      tags: ["e2e-history"],
      startAt: null,
    });
    [held] = await newIds(before, 1);
    ids.push(held);
    await invoke("add_tags", { ids: [held], tags: ["e2e-extra"] });
  });

  after(async () => {
    await clearRules();
    await removeAll(ids);
    await selectFilter("all");
  });

  it("shows what held a download and its tag change in its Log, with a type column", async () => {
    await selectFilter("all");
    await rowById(held).waitForExist();
    await dismissOverlays();
    await rowById(held).$("td.c-name").click();
    await $(`${PANE}[data-id="${held}"]`).waitForDisplayed();
    await $(`${PANE} button[role="tab"][data-tab="log"]`).click();
    await waitForText(LOG, "is closed until");
    await waitForText(LOG, "Added tag e2e-extra");
    await browser.waitUntil(async () => {
      const types = await $$(`${LOG} [data-testid="log-type"]`).map((t) => t.getText());
      return types.includes("Held") && types.includes("Tag");
    });
  });

  it("opens a rule's history from its menu and closes it with Escape", async () => {
    // The rule was saved behind the app's back, so reload to make the view read it.
    await browser.refresh();
    await $(SCHEDULES_LINK).click();
    await $(VIEW).waitForDisplayed();
    await $('[data-testid="rule-menu-button"]').click();
    await $('[data-testid="rule-menu"]').$("button=History").click();
    await $(DIALOG).waitForDisplayed();
    await waitForText(`${DIALOG} h2`, "E2E held: history");
    await waitForText(`${DIALOG} [data-testid="rule-history-empty"]`, "Nothing yet");
    await browser.keys([Key.Escape]);
    await waitForGone(DIALOG);
  });

  it("removes a download's events with it", async () => {
    expect((await invoke<Event[]>("download_history", { id: held })).map((e) => e.kind)).toEqual(
      expect.arrayContaining(["held", "tag_added"]),
    );
    await removeAll([held]);
    expect(await invoke<Event[]>("download_history", { id: held })).toEqual([]);
  });
});
