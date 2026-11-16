// Screenshots of named batches: the Add dialog with a batch name and folder tick, a
// pattern filling the name in, the grouped list with a group row, and the sidebar's
// Batches section. Run on its own with
// `pnpm screenshots --spec screenshots/batch.shots.ts`; RDM_BATCH_SHOTS_OUT picks the folder
// (default: the README's docs/screenshots). Not part of the e2e suite.

import { mkdirSync } from "node:fs";
import path from "node:path";
import { $, browser } from "@wdio/globals";
import {
  ADD_DIALOG,
  baseUrl,
  downloadDir,
  engineRows,
  retype,
  setDestination,
  waitForGone,
} from "../helpers";

const outDir = process.env.RDM_BATCH_SHOTS_OUT ?? process.env.RDM_SHOTS_OUT!;
const MB = 1000 * 1000;

/** A fixture URL serving `bytes` bytes (ranges supported) under the file name `name`. */
const file = (bytes: number, name: string) => `${baseUrl}/seg/${Math.round(bytes)}/${name}`;

async function paste(lines: string[]): Promise<void> {
  await browser.execute((text) => {
    const data = new DataTransfer();
    data.setData("text/plain", text);
    document.body.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
    );
  }, lines.join("\n"));
  await $(ADD_DIALOG).waitForDisplayed();
}

async function shoot(name: string): Promise<void> {
  // Finishes keep raising notifications while the batches run; hide them rather than chase them.
  await browser.execute(() => {
    if (document.getElementById("shots-hide")) return;
    const style = document.createElement("style");
    style.id = "shots-hide";
    style.textContent =
      '[data-testid="finish-notification"], [data-testid="toast"] { display: none !important; }';
    document.head.append(style);
  });
  await browser.execute(() => (document.activeElement as HTMLElement | null)?.blur());
  await browser.pause(600);
  await browser.saveScreenshot(path.join(outDir, `${name}.png`));
}

describe("batch screenshots", () => {
  before(async () => {
    mkdirSync(outDir, { recursive: true });
    await browser.setWindowSize(1280, 900);
    await browser.execute(() => window.dispatchEvent(new Event("resize")));
    await browser.pause(500);
    await $('[data-testid="add-button"]').waitForDisplayed();
  });

  it("captures the Add dialog, the grouped list and the Batches section", async () => {
    // A pattern on its own: the name fills in, the range opens, "9 more" reveals the rest.
    await paste([file(14 * MB, "lesson[01-12].mp4")]);
    const dialog = $(ADD_DIALOG);
    await setDestination(downloadDir);
    await dialog.$('button[aria-label^="Show lesson01.mp4"]').click();
    await shoot("batch-add-pattern-2");
    await dialog.$("button=9 more").click();
    await shoot("batch-add-pattern-show-less");
    await dialog.$("button=Cancel").click();
    await waitForGone(ADD_DIALOG);

    // A list with a name and the folder tick.
    await paste([
      "# course files",
      `${file(14 * MB, "lesson[01-12].mp4")},${file(0.3 * MB, "slides.pdf")}`,
      `${file(0.3 * MB, "slides.pdf")}|${file(0.2 * MB, "notes.txt")}`,
      `${baseUrl}/missing/lesson13.mp4`,
      `${baseUrl}/missing/lesson14.mp4`,
      "not a link",
    ]);
    await setDestination(downloadDir);
    await retype(dialog.$('input[aria-label="Batch name"]'), "Course files", { enter: false });
    await shoot("batch-add-named-2");
    await dialog.$('[data-testid="skipped-toggle"]').click();
    await shoot("batch-add-skipped-reasons");
    await dialog.$('[data-testid="skipped-toggle"]').click();
    await dialog.$("button*=Add").click();
    await waitForGone(ADD_DIALOG);

    // A second, smaller batch.
    await paste(["a", "b", "c", "d"].map((n) => file(2 * MB, `photo-${n}.jpg`)));
    await setDestination(downloadDir);
    await retype(dialog.$('input[aria-label="Batch name"]'), "Photos 15 Nov", { enter: false });
    await dialog.$("button*=Add").click();
    await waitForGone(ADD_DIALOG);

    // A few downloads in no batch: they close the list under Ungrouped.
    await paste([
      file(1 * MB, "setup.exe"),
      `${baseUrl}/missing/manual.pdf`,
      file(1 * MB, "song.mp3"),
    ]);
    await setDestination(downloadDir);
    await dialog.$("button*=Add").click();
    await waitForGone(ADD_DIALOG);

    await browser.waitUntil(
      async () => {
        const rows = await engineRows();
        return (
          rows.filter((d) => d.status === "completed").length >= 6 &&
          rows.filter((d) => d.status === "failed").length === 3
        );
      },
      { timeout: 120_000, interval: 500, timeoutMsg: "the batches never got going" },
    );
    await browser.execute(() =>
      document.querySelector("tbody tr:last-child")?.scrollIntoView({ block: "end" }),
    );
    await shoot("batch-grouped-list-3");

    await $('button[data-testid="batch-filter"][data-count="16"]').click();
    await shoot("batch-sidebar-view-3");
  });
});
