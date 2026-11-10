// End-to-end: troubleshooting. The settings menu's Open logs folder opens the
// log folder (through the `RDM_OPEN_LOG` seam) and Copy diagnostics puts a
// report on the (stubbed) clipboard with the version line and the logs, a
// download's secret query redacted. A render error (raised through the
// window's test hook) brings up the fallback screen instead of a blank
// window: the error waits behind Show details, its Copy diagnostics adds the
// error, the app log gets an `rdm::frontend` ERROR line, and Reload brings the
// window back.

import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { $, browser, expect } from "@wdio/globals";
import {
  addThroughDialog,
  baseUrl,
  clipboardWrites,
  openLog,
  removeAll,
  restoreClipboard,
  settingsButton,
  stubClipboard,
  TABLE,
  waitForGone,
  waitForStatus,
  waitForText,
} from "../helpers";

const MENU = '[role="dialog"][aria-label="Settings"]';
const OPEN_LOGS = `${MENU} [data-testid="open-logs-folder"]`;
const COPY_DIAGNOSTICS = `${MENU} [data-testid="copy-diagnostics"]`;
const FALLBACK = '[data-testid="fallback"]';
/** The window's test hook (see RenderErrorProbe.tsx). */
const RENDER_ERROR_EVENT = "rdm-test-render-error";
const RENDER_ERROR = "e2e render error";

/** Every daily log file in the app's state folder, oldest first, as one text. */
function appLog(): string {
  const dir = process.env.RDM_HOME!;
  return readdirSync(dir)
    .filter((f) => /^rdm\..*\.log$/.test(f))
    .sort()
    .map((f) => readFileSync(path.join(dir, f), "utf8"))
    .join("");
}

/** Clicks a Copy diagnostics button and returns what it put on the clipboard. */
async function copyDiagnostics(selector: string): Promise<string> {
  const before = (await clipboardWrites()).length;
  await $(selector).click();
  await browser.waitUntil(async () => (await clipboardWrites()).length > before, {
    timeoutMsg: "Copy diagnostics never wrote to the clipboard",
  });
  await waitForText(selector, "Copied");
  return (await clipboardWrites()).at(-1)!;
}

async function openSettings(): Promise<void> {
  if (!(await $(MENU).isExisting())) await settingsButton().click();
  await $(MENU).waitForDisplayed();
}

async function closeSettings(): Promise<void> {
  if (await $(MENU).isExisting()) await settingsButton().click();
  await waitForGone(MENU);
}

describe("rdm GUI troubleshooting", () => {
  const ids: number[] = [];

  before(async () => {
    await stubClipboard();
  });

  after(async () => {
    try {
      await closeSettings();
      await restoreClipboard();
    } finally {
      await removeAll(ids);
    }
  });

  it("opens the logs folder from the settings menu", async () => {
    const home = process.env.RDM_HOME!;
    await openSettings();
    await expect($(OPEN_LOGS)).toHaveAttribute("title", home);
    const before = openLog().length;
    await $(OPEN_LOGS).click();
    await browser.waitUntil(() => openLog().length > before, {
      timeoutMsg: "Open logs folder opened nothing",
    });
    expect(openLog().at(-1)).toBe(`open ${home}`);
  });

  it("copies diagnostics with the version line and no download secrets", async () => {
    await closeSettings();
    const id = await addThroughDialog(`${baseUrl}/file?token=SECRET`);
    ids.push(id);
    await waitForStatus(id, "completed", 30_000);
    // The file log is written on its own thread; wait for the download's lines.
    await browser.waitUntil(() => appLog().includes("?<redacted>"), {
      timeoutMsg: "the download never reached the app log",
    });

    await openSettings();
    const text = await copyDiagnostics(COPY_DIAGNOSTICS);
    expect(text).toMatch(/^rdm diagnostics\nrdm \d+\.\d+\.\d+.* · WebView .* · memory /);
    expect(text).toContain("Settings: ");
    expect(text).toContain("?<redacted>");
    expect(text).not.toContain("SECRET");
    expect(text).toMatch(/Last \d+ log lines:/);
    await expect($(`${MENU} .ts-hint`)).toHaveText(
      "Links in diagnostics have passwords and query strings removed.",
    );
  });

  it("shows the fallback screen when drawing the window fails, and logs it", async () => {
    await closeSettings();
    await browser.execute(
      (event, message) => window.dispatchEvent(new CustomEvent(event, { detail: message })),
      RENDER_ERROR_EVENT,
      RENDER_ERROR,
    );
    await $(FALLBACK).waitForDisplayed({ timeoutMsg: "no fallback screen" });
    await expect($(TABLE)).not.toBeExisting();
    await expect($(`${FALLBACK} h1`)).toHaveText("Something went wrong");

    // The technical text waits behind Show details.
    // Read from the page, not the driver's "displayed" check, which WebKitGTK's
    // driver may not apply to a closed <details>.
    const open = () =>
      browser.execute(
        (sel) => document.querySelector<HTMLDetailsElement>(sel)!.open,
        `${FALLBACK} details`,
      );
    expect(await open()).toBe(false);
    await $(`${FALLBACK} summary`).click();
    expect(await open()).toBe(true);
    await waitForText(`${FALLBACK} [data-testid="fallback-error"]`, `Error: ${RENDER_ERROR}`);

    const text = await copyDiagnostics(`${FALLBACK} [data-testid="copy-diagnostics"]`);
    expect(text).toMatch(/^rdm diagnostics\nrdm \d+\.\d+\.\d+/);
    expect(text).toContain(`Window error: Error: ${RENDER_ERROR}`);
    expect(text).toContain("Component stack:");

    await browser.waitUntil(
      () => appLog().includes(`ERROR rdm::frontend: render: Error: ${RENDER_ERROR}`),
      { timeoutMsg: "the render error never reached the app log" },
    );

    await $(FALLBACK).$("button=Reload").click();
    await $(TABLE).waitForDisplayed({ timeoutMsg: "Reload didn't bring the window back" });
    // A reload drops the stubbed clipboard; the after hook's restore is harmless.
    await stubClipboard();
  });
});
