// End-to-end: connection ramping. A multi-segment download starts on one
// connection and adds more while its measured speed keeps rising; the row
// shows the live count (Connections cell + `data-connections`) while it
// downloads, and drops it once paused. The fixture slows each /seg connection
// (see wdio.conf.ts) so the download outlasts the engine's 1 s ramp windows.

import { $, browser, expect } from "@wdio/globals";
import {
  addThroughDialog,
  attr,
  baseUrl,
  clickRowAction,
  removeAll,
  rowSelector,
  selectFilter,
  waitForGone,
  waitForStatus,
} from "../helpers";

const connectionsSelector = (id: number) => `${rowSelector(id)} [data-testid="row-connections"]`;

/** The row's live connection count (its `data-connections`), or 0 while it shows none. */
async function shownConnections(id: number): Promise<number> {
  // A row redrawn mid-read reads as "none yet".
  try {
    const n = Number(await attr(id, "data-connections"));
    return Number.isFinite(n) ? n : 0;
  } catch {
    return 0;
  }
}

describe("rdm GUI connection ramping", () => {
  let id: number | undefined;

  after(async () => {
    if (id !== undefined) await removeAll([id]);
  });

  it("adds connections while speed rises and shows the live count", async () => {
    await selectFilter("all");
    id = await addThroughDialog(`${baseUrl}/seg`);
    const own = id;

    await waitForStatus(own, "downloading", 30_000);
    // The first ramp window is skipped and each later one is 1 s, so allow
    // plenty of slack for a loaded runner.
    await browser.waitUntil(async () => (await shownConnections(own)) >= 2, {
      timeout: 20_000,
      interval: 200,
      timeoutMsg: `download ${own} never showed 2+ connections`,
    });
    // The visible Connections cell renders the count too (textContent, so a
    // column hidden at a narrow width doesn't matter).
    await browser.waitUntil(
      async () => {
        try {
          const text = await $(connectionsSelector(own)).getProperty("textContent");
          return Number(text) >= 2;
        } catch {
          return false;
        }
      },
      { timeout: 10_000, interval: 200, timeoutMsg: `download ${own}'s Connections cell is empty` },
    );

    await clickRowAction(own, "Pause");
    await waitForStatus(own, "paused");
    await waitForGone(connectionsSelector(own), `download ${own} kept its connection count`);
    expect(await attr(own, "data-connections")).toBe("0");
  });
});
