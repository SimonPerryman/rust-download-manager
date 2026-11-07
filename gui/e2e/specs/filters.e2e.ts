// End-to-end: sidebar filters. With a completed, a failed, a downloading and
// a waiting download (in two folders), each status filter shows exactly the
// rows of its status and its live count matches; folder filters split rows by
// target folder; the name filter narrows by filename. Specs share one state
// dir, so counts are compared with what the DOM shows, never with constants.

import { mkdirSync, rmSync } from "node:fs";
import path from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import {
  ROW,
  addThroughDialog,
  baseUrl,
  downloadDir,
  filterCount,
  folderButton,
  removeAll,
  retype,
  selectFilter,
  setMaxConcurrent,
  shownIds,
  waitForStatus,
} from "../helpers";

const FILTERS: { key: string; title: string; statuses: string[] }[] = [
  { key: "all", title: "All downloads", statuses: [] },
  { key: "downloading", title: "Downloading", statuses: ["downloading"] },
  { key: "waiting", title: "Waiting", statuses: ["queued"] },
  { key: "paused", title: "Paused", statuses: ["paused"] },
  { key: "done", title: "Done", statuses: ["completed"] },
  { key: "failed", title: "Failed", statuses: ["failed"] },
  { key: "stopped", title: "Stopped", statuses: ["cancelled"] },
];

const nameFilter = () => $('input[aria-label="Filter by name"]');

/** The first integer in the toolbar's view count. */
async function viewCount(): Promise<number> {
  const match = /\d+/.exec(await $('[data-testid="view-count"]').getText());
  return match ? Number(match[0]) : 0;
}

/** Every shown row's status. */
async function shownStatuses(): Promise<string[]> {
  return browser.execute(
    (sel) => Array.from(document.querySelectorAll(sel)).map((r) => r.getAttribute("data-status")!),
    ROW,
  );
}

/** Every shown row's filename. */
async function shownFilenames(): Promise<string[]> {
  return browser.execute(
    (sel) =>
      Array.from(document.querySelectorAll(sel)).map((r) => r.getAttribute("data-filename")!),
    ROW,
  );
}

describe("rdm GUI filters", () => {
  const other = path.join(downloadDir, "other");
  let completed = 0;
  let failed = 0;
  let downloading = 0;
  let queued = 0;

  before(async () => {
    mkdirSync(other, { recursive: true });
    await selectFilter("all");

    completed = await addThroughDialog(`${baseUrl}/file`);
    await waitForStatus(completed, "completed", 30_000);
    failed = await addThroughDialog(`${baseUrl}/seg-403`);
    await waitForStatus(failed, "failed", 30_000);

    // /gated never finishes on its own, so with a cap of 1 these two hold
    // their statuses for the whole spec.
    await setMaxConcurrent(1);
    downloading = await addThroughDialog(`${baseUrl}/gated`);
    await waitForStatus(downloading, "downloading");
    queued = await addThroughDialog(`${baseUrl}/gated`, other);
    await waitForStatus(queued, "queued");
  });

  after(async () => {
    try {
      if (await $('button[aria-label="Clear filter"]').isExisting()) {
        await $('button[aria-label="Clear filter"]').click();
      }
      await selectFilter("all");
      await setMaxConcurrent(3);
    } finally {
      await removeAll([completed, failed, downloading, queued].filter((id) => id !== 0));
      rmSync(other, { recursive: true, force: true });
    }
  });

  it("shows exactly each status's rows under its filter, with matching counts", async () => {
    const ours = { completed, failed, downloading, queued };
    const expected: Record<string, number[]> = {
      all: [completed, failed, downloading, queued],
      downloading: [downloading],
      waiting: [queued],
      paused: [],
      done: [completed],
      failed: [failed],
      stopped: [],
    };

    for (const { key, title, statuses } of FILTERS) {
      await selectFilter(key);
      await expect($("h1")).toHaveText(title, { containing: true });
      await browser.waitUntil(
        async () => {
          const shown = (await shownIds()).length;
          return (await filterCount(key)) === shown && (await viewCount()) === shown;
        },
        { timeoutMsg: `the ${key} filter's count doesn't match the rows it shows` },
      );
      if (statuses.length) {
        for (const status of await shownStatuses()) expect(statuses).toContain(status);
      }
      const shown = await shownIds();
      for (const id of Object.values(ours)) {
        expect(shown.includes(id)).toBe(expected[key].includes(id));
      }
      if (shown.length === 0) {
        await expect($('[data-testid="empty-state"]')).toBeExisting();
      }
    }
    await selectFilter("all");
  });

  it("splits downloads by target folder", async () => {
    const main = await folderButton(downloadDir);
    expect(main).toBeDefined();
    await main!.click();
    await expect(main!).toHaveAttribute("aria-current", "true");
    await browser.waitUntil(
      async () => Number(await main!.getAttribute("data-count")) === (await shownIds()).length,
      { timeoutMsg: "the download folder's count doesn't match the rows it shows" },
    );
    const inMain = await shownIds();
    for (const id of [completed, failed, downloading]) expect(inMain).toContain(id);
    expect(inMain).not.toContain(queued);

    const second = await folderButton(other);
    expect(second).toBeDefined();
    await second!.click();
    await expect(second!).toHaveAttribute("aria-current", "true");
    await expect(second!).toHaveAttribute("data-count", "1");
    await browser.waitUntil(async () => (await shownIds()).join() === String(queued), {
      timeoutMsg: `the ${other} folder doesn't show exactly download ${queued}`,
    });

    await selectFilter("all");
  });

  it("narrows the list by name and clears the name filter", async () => {
    await selectFilter("all");
    await retype(nameFilter(), "gated", { enter: false });
    await browser.waitUntil(
      async () => {
        const names = await shownFilenames();
        const ids = await shownIds();
        return (
          names.every((n) => n.toLowerCase().includes("gated")) &&
          ids.includes(downloading) &&
          ids.includes(queued)
        );
      },
      { timeoutMsg: "the name filter didn't narrow the list to the gated downloads" },
    );
    expect(await shownIds()).not.toContain(completed);

    await retype(nameFilter(), "zzq-no-match", { enter: false });
    await expect($('[data-testid="empty-state"]')).toHaveText("Nothing matches", {
      containing: true,
    });
    expect(await $$(ROW).length).toBe(0);

    await $('button[aria-label="Clear filter"]').click();
    await expect(nameFilter()).toHaveValue("");
    await browser.waitUntil(
      async () => {
        const ids = await shownIds();
        return [completed, failed, downloading, queued].every((id) => ids.includes(id));
      },
      { timeoutMsg: "clearing the name filter didn't bring every row back" },
    );
  });
});
