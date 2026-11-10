// End-to-end: the real app, driven through its webview, against the local
// fixture server (see wdio.conf.ts). Specs run in order and share one app
// state dir, so later specs can see earlier downloads in the list.

import { existsSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { $, browser, expect } from "@wdio/globals";
import {
  ADD_DIALOG,
  LINKS,
  addThroughDialog,
  attr,
  baseUrl,
  clickRowAction,
  downloadDir,
  removeAll,
  rowSelector,
  selectFilter,
  stopThroughDialog,
  waitForGone,
  waitForPartBytes,
  waitForStatus,
} from "../helpers";

/** The fixture's deterministic payload (mirrors `payload()` in tests/common). */
const PAYLOAD = Buffer.from(Array.from({ length: 99_991 }, (_, i) => i % 251));

const addDialog = () => $(ADD_DIALOG);

/** Pauses `id` once some bytes have landed, so the pause is mid-stream. */
async function pauseMidStream(id: number): Promise<void> {
  await waitForStatus(id, "downloading");
  await waitForPartBytes(id);
  await clickRowAction(id, "Pause");
  await waitForStatus(id, "paused");
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

/** Waits for the finished file, checks its bytes, and finds it completed under All. */
async function expectCompleted(id: number, filename: string): Promise<void> {
  const file = path.join(downloadDir, filename);
  await browser.waitUntil(() => existsSync(file), {
    timeoutMsg: `${file} never appeared`,
  });
  expect(readFileSync(file).equals(PAYLOAD)).toBe(true);
  expect(existsSync(`${file}.part`)).toBe(false);

  await selectFilter("all");
  await waitForStatus(id, "completed");
}

describe("rdm GUI", () => {
  const ids: number[] = [];

  after(async () => {
    await removeAll(ids);
  });

  it("adds a download, shows its Content-Disposition name, pauses, resumes and completes", async () => {
    const id = await addThroughDialog(`${baseUrl}/named-slow`);
    ids.push(id);
    expect(await attr(id, "data-filename")).toBe("report-final.bin");

    await pauseMidStream(id);
    await expectPausedMidStream("report-final.bin");

    await clickRowAction(id, "Resume");
    await expectCompleted(id, "report-final.bin");
  });

  it("restores a paused download after an app restart and completes it", async () => {
    const id = await addThroughDialog(`${baseUrl}/slow`);
    ids.push(id);
    expect(await attr(id, "data-filename")).toBe("slow");
    await pauseMidStream(id);

    await browser.reloadSession();

    await waitForStatus(id, "paused");
    await expectPausedMidStream("slow");
    await clickRowAction(id, "Resume");
    await expectCompleted(id, "slow");
  });

  // Runs after the restore spec, so `slow` is taken and this one is claimed as `slow (1)`.
  it("stops a running download deleting its partial, then archives it", async () => {
    const name = "slow (1)";
    const target = path.join(downloadDir, name);
    const id = await addThroughDialog(`${baseUrl}/slow`);
    ids.push(id);
    expect(await attr(id, "data-filename")).toBe(name);
    await waitForStatus(id, "downloading");
    await browser.waitUntil(() => existsSync(`${target}.part`), {
      timeoutMsg: `${target}.part never appeared`,
    });

    await stopThroughDialog(id, "delete");
    await waitForStatus(id, "cancelled");
    await browser.waitUntil(() => !existsSync(`${target}.part`) && !existsSync(`${target}.rdm`), {
      timeoutMsg: `stop and delete left partial state for ${name}`,
    });
    expect(existsSync(target)).toBe(false);

    await clickRowAction(id, "Archive");
    await waitForGone(rowSelector(id));
  });

  it("pauses everything with Pause all and resumes it with Resume all", async () => {
    const first = await addThroughDialog(`${baseUrl}/crawl`);
    ids.push(first);
    const second = await addThroughDialog(`${baseUrl}/crawl`);
    ids.push(second);
    await waitForStatus(first, "downloading");
    await waitForStatus(second, "downloading");

    await $("button=Pause all").click();
    await waitForStatus(first, "paused");
    await waitForStatus(second, "paused");

    const resumeAll = $("button=Resume all");
    await resumeAll.waitForDisplayed();
    await resumeAll.click();
    // Either may wait for a slot, so "left paused" is the assertion, not "downloading".
    await browser.waitUntil(
      async () => {
        const states = [await attr(first, "data-status"), await attr(second, "data-status")];
        return states.every((s) => s === "downloading" || s === "queued");
      },
      { timeoutMsg: "Resume all didn't resume both paused downloads" },
    );
    await removeAll([first, second]);
  });

  // /huge advertises 1023.9 MiB, the widest string the Size column formats.
  it("fits a large size in the Size column while downloading and paused", async () => {
    const id = await addThroughDialog(`${baseUrl}/huge`);
    ids.push(id);
    // Not getText(): the cell ellipsises, and WebKitGTK's driver reads that as "".
    const sizeCell = () =>
      browser.execute((sel) => {
        const cell = document.querySelector(`${sel} td.c-size`) as HTMLElement | null;
        return cell
          ? { text: cell.innerText, scroll: cell.scrollWidth, client: cell.clientWidth }
          : null;
      }, rowSelector(id));
    const expectFits = async () => {
      const cell = await sizeCell();
      expect(cell?.text).toBe("1023.9 MB");
      expect(cell!.scroll).toBeLessThanOrEqual(cell!.client);
    };

    await waitForStatus(id, "downloading");
    await browser.waitUntil(async () => (await sizeCell())?.text !== "", {
      timeoutMsg: "the Size cell never showed the advertised length",
    });
    await expectFits();

    await clickRowAction(id, "Pause");
    await waitForStatus(id, "paused");
    await expectFits();
    await removeAll([id]);
  });

  // Across a reload, not a restart: msedgedriver gives every session a fresh
  // throwaway webview profile, so localStorage can't outlive a session there.
  it("keeps the theme choice across a reload, from Settings and from the toolbar toggle", async () => {
    const rootState = () =>
      browser.execute(() => ({
        theme: document.documentElement.dataset.theme,
        dark: document.documentElement.classList.contains("dark"),
      }));

    const settings = $('button[aria-label="Settings"]');
    await settings.click();
    await $('[role="group"][aria-label="Theme"]').$("button=Dark").click();
    await settings.click();
    expect(await rootState()).toEqual({ theme: "dark", dark: true });

    await browser.refresh();
    await $('[data-testid="add-button"]').waitForDisplayed();
    expect(await rootState()).toEqual({ theme: "dark", dark: true });

    const toggle = $('button[aria-label^="Switch to"]');
    await expect(toggle).toHaveAttribute("aria-label", "Switch to light theme");
    await toggle.click();
    expect(await rootState()).toEqual({ theme: "light", dark: false });
    await expect(toggle).toHaveAttribute("aria-label", "Switch to dark theme");

    await browser.refresh();
    await $('[data-testid="add-button"]').waitForDisplayed();
    expect(await rootState()).toEqual({ theme: "light", dark: false });
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
    await expect(dialog.$(LINKS)).toHaveValue(url);
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
    await expect(dialog.$(LINKS)).toHaveValue(url);
    await dialog.$("button=Cancel").click();
    await waitForGone(ADD_DIALOG);
  });
});
