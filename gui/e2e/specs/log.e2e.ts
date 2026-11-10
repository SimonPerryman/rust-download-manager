// End-to-end: the details pane's Log tab. A download that fails against
// /seg-403 shows its engine lines, the error among them, in its Log tab; the
// Detailed bar never covers a line, scrolled or not; the tab follows the
// details pane to another download (a completed one ends on its Completed
// summary), the Detailed toggle adds the per-request lines and takes them
// away again, it says so when a download hasn't logged anything (one waiting
// in the queue), keeps polling while it's open (a running download's pause
// shows up without reopening it), Copy puts the lines shown on the clipboard
// (through a stubbed clipboard: WebKitGTK's driver can't read the real one),
// and the list goes away when another tab is chosen or the pane closes.

import { $, $$, browser, expect } from "@wdio/globals";
import {
  addThroughDialog,
  baseUrl,
  dismissOverlays,
  invoke,
  removeAll,
  rowById,
  selectFilter,
  waitForGone,
  waitForStatus,
  waitForText,
} from "../helpers";

const PANE = '[data-testid="details-pane"]';
const paneSelector = (id: number) => `${PANE}[data-id="${id}"]`;
const LOG = `${PANE} [data-testid="log"]`;
const DETAILED = `${PANE} [data-testid="log-detailed"]`;
const COPY = `${PANE} [data-testid="log-copy"]`;
/** Where the Log tab keeps the Detailed choice (see gui/src/lib/prefs.ts). */
const LOG_DETAILED_KEY = "rdm.gui.logDetailed";
const tab = (key: string) => $(`${PANE} button[role="tab"][data-tab="${key}"]`);

const BODY = `${PANE} .pane-body`;

interface BarGeometry {
  /** The scroller's padding-box top: where its visible area starts. */
  scrollerTop: number;
  barTop: number;
  barBottom: number;
  firstLineTop: number;
  firstLineBottom: number;
}

/** Where the Detailed bar and the lines sit inside the scrolling pane body. */
async function barGeometry(): Promise<BarGeometry> {
  return browser.execute(
    (body, detailed) => {
      const scroller = document.querySelector(body) as HTMLElement;
      const bar = (document.querySelector(detailed) as HTMLElement).closest(".log-bar")!;
      const barRect = bar.getBoundingClientRect();
      const scrollerTop = scroller.getBoundingClientRect().top + scroller.clientTop;
      const line = (scroller.querySelector("ol li") as HTMLElement).getBoundingClientRect();
      return {
        scrollerTop,
        barTop: barRect.top,
        barBottom: barRect.bottom,
        firstLineTop: line.top,
        firstLineBottom: line.bottom,
      };
    },
    BODY,
    DETAILED,
  );
}

/**
 * Swaps the clipboard for one that records what is written, so a spec can
 * check what Copy wrote; `restoreClipboard` puts the real one back.
 */
async function stubClipboard(): Promise<void> {
  await browser.execute(() => {
    const w = window as unknown as { copied: string[] };
    w.copied = [];
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: async (text: string) => void w.copied.push(text) },
    });
  });
}

async function restoreClipboard(): Promise<void> {
  await browser.execute(() => {
    delete (navigator as unknown as { clipboard?: unknown }).clipboard;
  });
}

/** Clicks Copy and returns what it wrote, once the button says so. */
async function copyLines(): Promise<{ text: string; shown: number; said: string }> {
  const before = await browser.execute(
    () => (window as unknown as { copied: string[] }).copied.length,
  );
  await $(COPY).click();
  await browser.waitUntil(
    () =>
      browser.execute((n) => (window as unknown as { copied: string[] }).copied.length > n, before),
    { timeoutMsg: "Copy never wrote to the clipboard" },
  );
  await waitForText(COPY, "Copied");
  return browser.execute(
    (log, pane) => ({
      text: (window as unknown as { copied: string[] }).copied.at(-1) ?? "",
      shown: document.querySelectorAll(`${log} li`).length,
      said: document.querySelector(`${pane} .log-bar [role="status"]`)?.textContent ?? "",
    }),
    LOG,
    PANE,
  );
}

/** Opens the details pane for `id` by clicking its row (outside the buttons). */
async function openDetails(id: number): Promise<void> {
  await dismissOverlays();
  await rowById(id).$("td.c-name").click();
  await $(paneSelector(id)).waitForDisplayed({ timeoutMsg: `no details pane for ${id}` });
}

describe("rdm GUI log tab", () => {
  const ids: number[] = [];
  let failed = 0;
  let done = 0;
  let running = 0;

  before(async () => {
    // A run that died with Detailed on would otherwise start with it on.
    await browser.execute((key) => localStorage.removeItem(key), LOG_DETAILED_KEY);
    await selectFilter("all");
    failed = await addThroughDialog(`${baseUrl}/seg-403`);
    ids.push(failed);
    done = await addThroughDialog(`${baseUrl}/file`);
    ids.push(done);
    // Held open by the fixture's gate, so it's still running when its Log opens.
    running = await addThroughDialog(`${baseUrl}/gated`);
    ids.push(running);
    await waitForStatus(failed, "failed", 30_000);
    await waitForStatus(done, "completed", 30_000);
  });

  after(async () => {
    try {
      if (await $(PANE).isExisting()) {
        await $(`${PANE} button[aria-label="Close details"]`).click();
        await waitForGone(PANE);
      }
      await browser.execute((key) => localStorage.removeItem(key), LOG_DETAILED_KEY);
    } finally {
      await removeAll(ids);
    }
  });

  it("shows a failed download's error lines in its Log tab", async () => {
    await openDetails(failed);
    await tab("log").click();
    await expect(tab("log")).toHaveAttribute("aria-selected", "true");
    await waitForText(LOG, "Started · 4.00 MB · resumable");
    await waitForText(LOG, "Failed");
    await waitForText(LOG, "server returned HTTP 403");
    await expect($(`${LOG} li[data-level="error"]`)).toBeExisting();
  });

  it("keeps the Detailed bar clear of a one-line log", async () => {
    const missing = await addThroughDialog(`${baseUrl}/missing`);
    ids.push(missing);
    await waitForStatus(missing, "failed", 30_000);
    await openDetails(missing);
    await expect(tab("log")).toHaveAttribute("aria-selected", "true");
    await waitForText(LOG, "server returned HTTP 404");
    const g = await barGeometry();
    // Below the bar, not under it: the two rects don't intersect.
    expect(g.firstLineTop).toBeGreaterThanOrEqual(g.barBottom);
    expect(g.barTop).toBeCloseTo(g.scrollerTop, 0);
  });

  it("follows the details pane to another download", async () => {
    await openDetails(done);
    await expect(tab("log")).toHaveAttribute("aria-selected", "true");
    await waitForText(LOG, "Saved to ");
    await waitForText(LOG, "Completed · 97.6 KB in ");
    await waitForGone(`${LOG} li[data-level="error"]`, "the other download's error stayed");
  });

  it("pins the Detailed bar flush to the top of a scrolled log", async () => {
    // Shrink the pane body so even this short log scrolls.
    const scrolled = await browser.execute((body) => {
      const scroller = document.querySelector(body) as HTMLElement;
      scroller.style.flex = "none";
      scroller.style.height = "90px";
      scroller.scrollTop = 20;
      return { top: scroller.scrollTop, max: scroller.scrollHeight - scroller.clientHeight };
    }, BODY);
    try {
      expect(scrolled.max).toBeGreaterThan(20);
      expect(scrolled.top).toBeGreaterThan(0);
      // Flush with the visible top: no band where lines show through above
      // it, and not sitting low over the lines.
      const g = await barGeometry();
      expect(g.barTop).toBeCloseTo(g.scrollerTop, 0);

      // Back at the top, the first line sits below the bar again. Retried:
      // a poll landing before the scroll event can follow the log's bottom.
      let top = g;
      await browser
        .waitUntil(
          async () => {
            await browser.execute((body) => {
              (document.querySelector(body) as HTMLElement).scrollTop = 0;
            }, BODY);
            top = await barGeometry();
            return top.firstLineTop >= top.barBottom;
          },
          { timeout: 5_000 },
        )
        .catch(() => undefined);
      expect(top.firstLineTop).toBeGreaterThanOrEqual(top.barBottom);
    } finally {
      await browser.execute((body) => {
        const scroller = document.querySelector(body) as HTMLElement;
        scroller.style.flex = "";
        scroller.style.height = "";
      }, BODY);
    }
  });

  it("adds the per-request lines while Detailed is on", async () => {
    expect(await $(DETAILED).isSelected()).toBe(false);
    expect(await $$(`${LOG} li[data-level="debug"]`).length).toBe(0);
    await $(DETAILED).click();
    try {
      await waitForText(LOG, "Requested the file · HTTP 200");
      expect(await $$(`${LOG} li[data-level="debug"]`).length).toBeGreaterThan(0);
    } finally {
      // Remembered across downloads (and launches), so leave it off.
      if (await $(DETAILED).isSelected()) await $(DETAILED).click();
    }
    await waitForGone(`${LOG} li[data-level="debug"]`, "the detail stayed after Detailed went off");
    await waitForText(LOG, "Completed · ");
  });

  it("copies the lines shown, as time and text, with or without the detail", async () => {
    await stubClipboard();
    try {
      const plain = await copyLines();
      const lines = plain.text.split("\n");
      expect(lines.length).toBe(plain.shown);
      for (const l of lines) expect(l).toMatch(/^\d\d:\d\d:\d\d {2}\S/);
      expect(plain.text).toContain("Completed · ");
      expect(plain.text).not.toContain("Requested the file");
      expect(plain.said).toBe(`Copied ${plain.shown} lines`);

      await $(DETAILED).click();
      try {
        await waitForText(LOG, "Requested the file · HTTP 200");
        const detailed = await copyLines();
        expect(detailed.text.split("\n").length).toBe(detailed.shown);
        expect(detailed.shown).toBeGreaterThan(plain.shown);
        expect(detailed.text).toContain("Requested the file · HTTP 200");
      } finally {
        if (await $(DETAILED).isSelected()) await $(DETAILED).click();
      }
      await waitForGone(
        `${LOG} li[data-level="debug"]`,
        "the detail stayed after Detailed went off",
      );
      // The label goes back once it has said so ("Copied" doesn't contain "Copy").
      await waitForText(COPY, "Copy", "Copy kept saying Copied");
    } finally {
      await restoreClipboard();
    }
  });

  it("drops the list when another tab is chosen", async () => {
    await tab("overview").click();
    await waitForGone(LOG, "the log stayed after leaving its tab");
    await tab("log").click();
    await waitForText(LOG, "Completed · ");
  });

  it("says nothing is logged yet for a download that hasn't run", async () => {
    await waitForStatus(running, "downloading", 30_000);
    // A cap of 1, held by the running download, keeps the next one waiting.
    await invoke("set_settings", { maxConcurrent: 1, globalLimit: null, maxConnections: null });
    try {
      const waiting = await addThroughDialog(`${baseUrl}/gated`);
      ids.push(waiting);
      await waitForStatus(waiting, "queued");
      await openDetails(waiting);
      await expect(tab("log")).toHaveAttribute("aria-selected", "true");
      await waitForText(`${PANE} [data-testid="log-empty"]`, "Nothing logged yet");
      expect(await $(LOG).isExisting()).toBe(false);
    } finally {
      await invoke("set_settings", { maxConcurrent: 3, globalLimit: null, maxConnections: null });
    }
  });

  it("keeps polling while open: a running download's new lines appear", async () => {
    await waitForStatus(running, "downloading", 30_000);
    await openDetails(running);
    await expect(tab("log")).toHaveAttribute("aria-selected", "true");
    await waitForText(LOG, "Started · size unknown · not resumable · 1 lane");
    await invoke("pause", { id: running });
    await waitForText(LOG, "Paused");
  });

  it("drops the list when the pane closes", async () => {
    await $(`${PANE} button[aria-label="Close details"]`).click();
    await waitForGone(LOG, "the log stayed after the pane closed");
    await waitForGone(PANE, "the details pane never closed");
  });
});
