// End-to-end: failed downloads. Downloads that fail against /seg-403 are
// pinned to the top of All above a newer completed one, each with its error
// inline in plain words; "Retry all failed" restarts every one of them. The
// restart is proved by `finished_at` changing (the engine clears it on retry
// and sets it again on the next failure), so the spec never has to catch the
// brief queued/downloading state in between.

import { $, browser, expect } from "@wdio/globals";
import {
  addThroughDialog,
  attr,
  baseUrl,
  dismissOverlays,
  errorRowSelector,
  removeAll,
  selectFilter,
  shownIds,
  waitForStatus,
} from "../helpers";

describe("rdm GUI failed downloads", () => {
  const ids: number[] = [];
  let f1 = 0;
  let f2 = 0;

  before(async () => {
    await selectFilter("all");
    f1 = await addThroughDialog(`${baseUrl}/seg-403`);
    ids.push(f1);
    f2 = await addThroughDialog(`${baseUrl}/seg-403`);
    ids.push(f2);
    await waitForStatus(f1, "failed", 30_000);
    await waitForStatus(f2, "failed", 30_000);
    // Newest, so only pinning can put the failed ones above it.
    const c = await addThroughDialog(`${baseUrl}/file`);
    ids.push(c);
    await waitForStatus(c, "completed", 30_000);
  });

  after(async () => {
    await selectFilter("all");
    await removeAll(ids);
  });

  it("pins failed downloads to the top of All with the error inline", async () => {
    const top = (await shownIds()).slice(0, 2);
    expect(new Set(top)).toEqual(new Set([f1, f2]));

    const next = await browser.execute((id) => {
      const row = document.querySelector(`tr[data-testid="download-row"][data-id="${id}"]`);
      const sibling = row?.nextElementSibling;
      return sibling
        ? { testid: sibling.getAttribute("data-testid"), id: sibling.getAttribute("data-id") }
        : null;
    }, f1);
    expect(next).toEqual({ testid: "download-error", id: String(f1) });

    const error = $(errorRowSelector(f1));
    await expect(error).toHaveText("refused the request (403 Forbidden)", { containing: true });
    await expect(error.$("button=Retry")).toBeExisting();
  });

  it("restarts every failed download with Retry all failed", async () => {
    const before = [await attr(f1, "data-finished-at"), await attr(f2, "data-finished-at")];
    expect(before.every((t) => t !== null && t !== "")).toBe(true);
    // Finish times are in seconds: make sure a re-failure lands in a later one.
    await browser.pause(1100);

    await dismissOverlays();
    await $(errorRowSelector(f1)).$("button=Retry all failed").click();
    await browser.waitUntil(
      async () => {
        const rows = [f1, f2].map(async (id, i) => {
          const at = await attr(id, "data-finished-at");
          const status = await attr(id, "data-status");
          return status === "failed" && at !== null && at !== "" && at !== before[i];
        });
        return (await Promise.all(rows)).every(Boolean);
      },
      { timeout: 30_000, timeoutMsg: "Retry all failed didn't restart both failed downloads" },
    );
  });

  it("offers Retry all failed in the Failed view's toolbar", async () => {
    await selectFilter("failed");
    await expect($(errorRowSelector(f1))).toBeExisting();
    await expect($(errorRowSelector(f2))).toBeExisting();
    // The toolbar's button, not the ones inside the error rows.
    await browser.waitUntil(
      () =>
        browser.execute(() =>
          Array.from(document.querySelectorAll("button")).some(
            (b) => b.textContent?.trim() === "Retry all failed" && b.closest("tr") === null,
          ),
        ),
      { timeoutMsg: "the Failed view's toolbar has no Retry all failed" },
    );
    await selectFilter("all");
  });
});
