// End-to-end: the real app, driven through its webview, against the local
// fixture server (see wdio.conf.ts). Specs run in order and share one app
// state dir, so later specs see earlier downloads in History.

import { existsSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { waitForGone } from "../helpers";

const baseUrl = process.env.RDM_E2E_BASE_URL!;
const downloadDir = process.env.RDM_DOWNLOAD_DIR!;

/** The fixture's deterministic payload (mirrors `payload()` in tests/common). */
const PAYLOAD = Buffer.from(Array.from({ length: 99_991 }, (_, i) => i % 251));

const rowSelector = (filename: string) =>
  `li[data-testid="download-row"][data-filename="${filename}"]`;
const ADD_DIALOG = '[role="dialog"][aria-label="Add download"]';

const row = (filename: string) => $(rowSelector(filename));
const addDialog = () => $(ADD_DIALOG);

async function waitForStatus(filename: string, status: string): Promise<void> {
  await browser.waitUntil(
    async () => {
      const r = row(filename);
      return (await r.isExisting()) && (await r.getAttribute("data-status")) === status;
    },
    { timeoutMsg: `${filename} never reached status "${status}"` },
  );
}

/** Pauses `filename` once some bytes have landed, so the pause is mid-stream. */
async function pauseMidStream(filename: string): Promise<void> {
  await waitForStatus(filename, "downloading");
  const part = path.join(downloadDir, `${filename}.part`);
  await browser.waitUntil(() => existsSync(part) && statSync(part).size > 0, {
    timeoutMsg: `${part} never received any bytes`,
  });
  await clickRowAction(filename, "Pause");
  await waitForStatus(filename, "paused");
}

async function clickRowAction(filename: string, action: string): Promise<void> {
  await row(filename).$(`button[aria-label="${action}"]`).click();
}

async function addThroughDialog(url: string): Promise<void> {
  await $("button=Add download").click();
  const dialog = addDialog();
  await dialog.waitForDisplayed();
  await expect(dialog.$('input[aria-label="Destination folder"]')).toHaveValue(downloadDir);
  await dialog.$('input[aria-label="URL"]').setValue(url);
  await dialog.$("button=Add download").click();
  await waitForGone(ADD_DIALOG);
}

/** Asserts a paused download really stopped mid-stream: a partial `.part` that isn't growing. */
async function expectPausedMidStream(filename: string): Promise<void> {
  const part = path.join(downloadDir, `${filename}.part`);
  const size = statSync(part).size;
  expect(size).toBeGreaterThan(0);
  expect(size).toBeLessThan(PAYLOAD.length);
  await browser.pause(800);
  expect(statSync(part).size).toBe(size);
  expect(existsSync(path.join(downloadDir, filename))).toBe(false);
}

/** Waits for the finished file, checks its bytes, and finds it Completed in History. */
async function expectCompleted(filename: string): Promise<void> {
  const file = path.join(downloadDir, filename);
  await browser.waitUntil(() => existsSync(file), {
    timeoutMsg: `${file} never appeared`,
  });
  expect(readFileSync(file).equals(PAYLOAD)).toBe(true);
  expect(existsSync(`${file}.part`)).toBe(false);

  await $("button*=History").click();
  await waitForStatus(filename, "completed");
  await $("button*=Active").click();
}

describe("rdm GUI", () => {
  it("adds a download, shows its Content-Disposition name, pauses, resumes and completes", async () => {
    await addThroughDialog(`${baseUrl}/named-slow`);

    await pauseMidStream("report-final.bin");
    await expectPausedMidStream("report-final.bin");

    await clickRowAction("report-final.bin", "Resume");
    await expectCompleted("report-final.bin");
  });

  it("restores a paused download after an app restart and completes it", async () => {
    await addThroughDialog(`${baseUrl}/slow`);
    await pauseMidStream("slow");

    await browser.reloadSession();

    await waitForStatus("slow", "paused");
    await expectPausedMidStream("slow");
    await clickRowAction("slow", "Resume");
    await expectCompleted("slow");
  });

  // Runs after the restore spec, so `slow` is taken and this one is claimed as `slow (1)`.
  it("cancels a running download, then removes it from History", async () => {
    const name = "slow (1)";
    const target = path.join(downloadDir, name);
    await addThroughDialog(`${baseUrl}/slow`);
    await waitForStatus(name, "downloading");
    await browser.waitUntil(() => existsSync(`${target}.part`), {
      timeoutMsg: `${target}.part never appeared`,
    });

    await clickRowAction(name, "Cancel");
    await $("button*=History").click();
    await waitForStatus(name, "cancelled");
    await browser.waitUntil(() => !existsSync(`${target}.part`) && !existsSync(`${target}.rdm`), {
      timeoutMsg: `cancel left partial state for ${name}`,
    });
    expect(existsSync(target)).toBe(false);

    await clickRowAction(name, "Remove");
    await waitForGone(rowSelector(name));
    await $("button*=Active").click();
  });

  // Across a reload, not a restart: msedgedriver gives every session a fresh
  // throwaway webview profile, so localStorage can't outlive a session there.
  it("keeps theme and density choices across a reload", async () => {
    const settings = $('button[aria-label="Settings"]');
    await settings.click();
    await $('[role="group"][aria-label="Theme"]').$("button=Dark").click();
    await $('[role="group"][aria-label="Density"]').$("button=Compact").click();
    await settings.click();

    const rootState = () =>
      browser.execute(() => ({
        theme: document.documentElement.dataset.theme,
        density: document.documentElement.dataset.density,
        dark: document.documentElement.classList.contains("dark"),
      }));
    expect(await rootState()).toEqual({ theme: "dark", density: "compact", dark: true });

    await browser.refresh();
    await $("button=Add download").waitForDisplayed();
    expect(await rootState()).toEqual({ theme: "dark", density: "compact", dark: true });
  });

  it("opens the Add dialog pre-filled when a URL is pasted", async () => {
    const url = `${baseUrl}/file`;
    await browser.execute((u) => {
      const data = new DataTransfer();
      data.setData("text/plain", u);
      document.dispatchEvent(
        new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
      );
    }, url);

    const dialog = addDialog();
    await dialog.waitForDisplayed();
    await expect(dialog.$('input[aria-label="URL"]')).toHaveValue(url);
    await dialog.$("button=Cancel").click();
    await waitForGone(ADD_DIALOG);
  });

  it("opens the Add dialog pre-filled when a URL is dropped", async () => {
    const url = `${baseUrl}/named`;
    await browser.execute((u) => {
      const data = new DataTransfer();
      data.setData("text/uri-list", u);
      document.body.dispatchEvent(
        new DragEvent("drop", { dataTransfer: data, bubbles: true, cancelable: true }),
      );
    }, url);

    const dialog = addDialog();
    await dialog.waitForDisplayed();
    await expect(dialog.$('input[aria-label="URL"]')).toHaveValue(url);
    await dialog.$("button=Cancel").click();
    await waitForGone(ADD_DIALOG);
  });
});
