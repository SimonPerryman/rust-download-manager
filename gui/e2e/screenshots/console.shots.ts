// The README screenshots: one session with downloads in every state (done,
// downloading, waiting, paused, failed), captured in the light and dark
// themes, then the details pane of a running download. Run with
// `pnpm screenshots` (see wdio.screenshots.conf.ts); not part of the e2e suite.

import { mkdirSync } from "node:fs";
import path from "node:path";
import { $, browser } from "@wdio/globals";
import {
  baseUrl,
  clickRowAction,
  dismissOverlays,
  downloadDir,
  invoke,
  rowById,
  rowSelector,
  settingsButton,
  waitForGone,
  waitForStatus,
} from "../helpers";

const outDir = process.env.RDM_SHOTS_OUT!;
const MB = 1000 * 1000;
const isoDir = path.join(downloadDir, "ISOs");

/** A fixture URL serving `bytes` bytes (ranges supported) under the file name `name`. */
const file = (bytes: number, name: string) => `${baseUrl}/seg/${Math.round(bytes)}/${name}`;

/** Adds a download (with `tags`) through the backend and waits for its row. */
async function add(url: string, dir = downloadDir, tags: string[] = []): Promise<number> {
  const id = await invoke<number>("add_download", { url, dir, tags });
  await rowById(id).waitForExist({ timeoutMsg: `${url} never showed a row` });
  return id;
}

/** Waits until download `id` has written at least `fraction` of its file. */
async function waitForProgress(id: number, fraction: number): Promise<void> {
  await browser.waitUntil(
    async () => {
      const rows = await invoke<{ id: number; downloaded: number; total: number | null }[]>("list");
      const d = rows.find((r) => r.id === id);
      return !!d && d.total != null && d.downloaded >= fraction * d.total;
    },
    { timeout: 120_000, interval: 100, timeoutMsg: `download ${id} never reached ${fraction}` },
  );
}

/** Picks a theme in Settings and closes the popover again. */
async function setTheme(theme: "Light" | "Dark"): Promise<void> {
  await settingsButton().click();
  const group = $('[role="group"][aria-label="Theme"]');
  await group.$(`button=${theme}`).click();
  await settingsButton().click();
  await waitForGone('[role="group"][aria-label="Theme"]');
}

/** Clears overlays, focus and hover, then saves the window as `name`.png. */
async function shoot(name: string): Promise<void> {
  await dismissOverlays();
  await browser.execute(() => (document.activeElement as HTMLElement | null)?.blur());
  await $("aside").moveTo({ xOffset: 0, yOffset: 200 });
  // Let hover and theme transitions settle.
  await browser.pause(600);
  await browser.saveScreenshot(path.join(outDir, `${name}.png`));
}

describe("README screenshots", () => {
  before(async () => {
    mkdirSync(outDir, { recursive: true });
    await browser.setWindowSize(1280, 720);
    // Let the webview re-lay itself out at the new size (seen offset under display scaling).
    await browser.execute(() => window.dispatchEvent(new Event("resize")));
    await browser.pause(500);
    await $('[data-testid="add-button"]').waitForDisplayed();
  });

  it("captures the main view in the light and dark themes", async () => {
    // Finished downloads.
    const done = [
      await add(file(29.6 * MB, "node-v22.11.0-x64.msi")),
      await add(file(73.6 * MB, "go1.23.3.linux-amd64.tar.gz"), downloadDir, ["dev"]),
      await add(file(44.1 * MB, "vlc-3.0.21-win64.exe")),
      await add(file(57.6 * MB, "Godot_v4.3-stable_win64.exe.zip"), downloadDir, ["dev"]),
      await add(file(88.2 * MB, "wallpapers-4k-2026.zip")),
    ];
    for (const id of done) await waitForStatus(id, "completed", 120_000);

    // An archived one, counted under Archived.
    const old = await add(file(12.4 * MB, "photos-2025.zip"), downloadDir, ["photos"]);
    await waitForStatus(old, "completed", 120_000);
    await invoke("archive", { ids: [old] });
    await waitForGone(rowSelector(old));

    // A failed one: the server answers 404.
    const failed = await add(`${baseUrl}/missing/photos-backup-2026-10.zip`);
    await waitForStatus(failed, "failed", 60_000);

    // A paused one, part-way through.
    const paused = await add(file(68.3 * MB, "Git-2.47.0-64-bit.exe"));
    await waitForProgress(paused, 0.37);
    await invoke("pause", { id: paused });
    await waitForStatus(paused, "paused");

    // Two downloading (the cap) and one waiting behind them.
    await invoke("set_settings", { maxConcurrent: 2 });
    const iso = await add(file(64.2 * MB, "alpine-virt-3.20.3-x86_64.iso"), isoDir);
    const dataset = await add(file(97.4 * MB, "dataset-2026.tar.gz"));
    const waiting = await add(file(92.5 * MB, "postgresql-17.2-1-windows-x64.exe"));
    await waitForStatus(iso, "downloading");
    await waitForStatus(dataset, "downloading");
    await waitForStatus(waiting, "queued");

    // Open the finished files but the last (logged, not launched), so only it keeps its
    // "not opened yet" highlight.
    for (const id of done.slice(0, -1)) await clickRowAction(id, "Open");

    await waitForProgress(iso, 0.4);
    await setTheme("Light");
    await shoot("main-light");
    await setTheme("Dark");
    await shoot("main-dark");

    // The Speed tab of a running download, once its graph has most of a minute. (Not
    // the Overview: it shows the fixture's URL and the screenshot run's own folder.)
    await waitForStatus(iso, "completed", 120_000);
    await clickRowAction(iso, "Open");
    await setTheme("Light");
    await rowById(dataset).$("td.c-name").click();
    const pane = `[data-testid="details-pane"][data-id="${dataset}"]`;
    await $(`${pane} button[role="tab"][data-tab="speed"]`).click();
    await $(`${pane} [data-testid="speed-graph"]`).waitForDisplayed();
    await waitForProgress(dataset, 0.85);
    await shoot("details");
  });
});
