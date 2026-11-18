// Screenshot of the Schedules view: a download window, a speed limit and a tag-scoped window in
// Rules, two downloads under Coming up, and the When downloads finish section. Run on its own
// with `pnpm screenshots --spec screenshots/schedules.shots.ts`; RDM_SHOTS_OUT picks the folder
// (default: the README's docs/screenshots). Not part of the e2e suite.

import { mkdirSync } from "node:fs";
import path from "node:path";
import { $, browser } from "@wdio/globals";
import { baseUrl, invoke, settingsButton, waitForGone } from "../helpers";

const outDir = process.env.RDM_SHOTS_OUT!;
const WEEKDAYS = ["mon", "tue", "wed", "thu", "fri"];
const ALL_DAYS = [...WEEKDAYS, "sat", "sun"];

describe("schedules screenshot", () => {
  before(async () => {
    mkdirSync(outDir, { recursive: true });
    await browser.setWindowSize(1280, 900);
    await browser.execute(() => window.dispatchEvent(new Event("resize")));
    await browser.pause(500);
    await $('[data-testid="add-button"]').waitForDisplayed();
  });

  it("captures the Schedules view", async () => {
    await invoke("set_schedule", {
      schedule: {
        queue_windows: [
          {
            name: "Overnight only",
            enabled: true,
            window: { start: "22:00", end: "08:00", days: ALL_DAYS },
            finish_running: false,
          },
          {
            name: "Work tag at night",
            enabled: true,
            window: { start: "01:00", end: "06:00", days: ALL_DAYS },
            finish_running: true,
            scope: { kind: "tag", tag: "work" },
          },
        ],
        speed_entries: [
          {
            name: "Daytime cap",
            enabled: true,
            window: { start: "08:00", end: "18:00", days: WEEKDAYS },
            limit: 5 * 1024 * 1024,
          },
        ],
      },
    });
    await invoke("set_post_actions", {
      actions: {
        command: { enabled: true, program: "scan.exe", args: ["{path}"] },
        power: { enabled: false, action: "shutdown" },
      },
    });
    for (const [name, startAt] of [
      ["ubuntu.iso", "22:00"],
      ["backup.zip", "03:00"],
    ]) {
      await invoke("add_download", {
        url: `${baseUrl}/seg/${10 * 1000 * 1000}/${name}`,
        dir: null,
        checksum: null,
        limit: null,
        tags: null,
        startAt,
      });
    }
    // invoke() doesn't wait for the command, and the view reads the rules when it opens: give
    // them a moment, then open the view.
    await browser.pause(1000);
    await settingsButton().click();
    const theme = $('[role="group"][aria-label="Theme"]');
    await theme.waitForDisplayed();
    await theme.$("button=Light").click();
    await settingsButton().click();
    await waitForGone('[role="group"][aria-label="Theme"]');
    // The page read its rules and actions at startup; reload so it shows the ones seeded above.
    await browser.refresh();
    await $('[data-testid="add-button"]').waitForDisplayed();
    await $('[data-testid="schedules-link"]').click();
    await $('[data-testid="schedules-view"]').waitForDisplayed();
    await browser.execute(() => {
      const style = document.createElement("style");
      style.textContent =
        '[data-testid="finish-notification"], [data-testid="toast"] { display: none !important; }';
      document.head.append(style);
      (document.activeElement as HTMLElement | null)?.blur();
    });
    await browser.pause(800);
    await browser.saveScreenshot(path.join(outDir, "schedules.png"));
  });
});
