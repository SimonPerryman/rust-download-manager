// End-to-end: the in-app updater against the local release feed (see
// updateFeed.ts), signed with a throwaway key; never GitHub. The debug
// build's test seam points the app at that feed and stops Restart now where
// the installer would run. Each check is counted by the feed, so a spec can
// tell when the app asked. Every other spec runs with automatic checks off;
// this one restores that, and the feed's offer, when it's done.

import { rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import {
  addThroughDialog,
  baseUrl,
  engineRow,
  openLog,
  removeAll,
  settingsButton,
  waitForGone,
  waitForStatus,
  waitForText,
} from "../helpers";
import { appVersion, newerVersion, offerFile } from "../updateFeed";

const NOTICE = '[data-testid="update-notice"]';
const BARS = '[data-testid="update-bars"]';
const PAUSED_BAR = '[data-testid="paused-for-update"]';
const FIRST_RUN = '[data-testid="first-run-card"]';
const RESULT = '[data-testid="update-check-result"]';
const AUTO_CHECK = 'input[aria-label="Check for updates automatically"]';

const feed = process.env.RDM_E2E_UPDATE_FEED!;
const prefsFile = path.join(process.env.RDM_HOME!, "updater.json");

/** How many times the app has asked the feed for its manifest. */
async function checks(): Promise<number> {
  const stats = (await (await fetch(`${feed}/stats`)).json()) as { manifests: number };
  return stats.manifests;
}

/** Make the feed offer `version` from its next request. */
const offer = (version: string) => writeFileSync(offerFile(process.env.RDM_E2E_ROOT!), version);

const setAutoCheckFile = (on: boolean) =>
  writeFileSync(prefsFile, JSON.stringify({ auto_check: on }));

/** Waits until the feed has answered `n` manifest requests in all. */
async function waitForChecks(n: number): Promise<void> {
  await browser.waitUntil(async () => (await checks()) >= n, {
    timeoutMsg: `the app never checked for updates (wanted ${n} checks)`,
  });
}

/**
 * Waits until the app has handled `n` checks this session (the update bars'
 * `data-checks-done`), so what it shows is the answer, not a stale screen.
 */
async function waitForHandledChecks(n: number): Promise<void> {
  await browser.waitUntil(async () => Number(await $(BARS).getAttribute("data-checks-done")) >= n, {
    timeoutMsg: `the app never handled ${n} update checks`,
  });
}

/** Opens Settings and waits until the updater's preferences have loaded. */
async function openSettings(): Promise<void> {
  await settingsButton().click();
  await expect($(AUTO_CHECK)).toBeEnabled();
}

describe("in-app updater", () => {
  const ids: number[] = [];

  after(async () => {
    // The shared state first: later specs must never see a check.
    setAutoCheckFile(false);
    offer(appVersion());
    await removeAll(ids);
  });

  it("checks nothing on start while automatic checks are off", async () => {
    setAutoCheckFile(false);
    const before = await checks();
    await browser.reloadSession();

    // Loaded preferences mean the start-up decision has been made.
    await openSettings();
    await expect($(AUTO_CHECK)).not.toBeSelected();
    await settingsButton().click();
    await waitForGone(AUTO_CHECK);
    await browser.pause(1_000);
    expect(await checks()).toBe(before);
    expect(await $$(FIRST_RUN).length).toBe(0);
  });

  it("asks once on a first start, checks nothing before the answer, then checks on Yes", async () => {
    offer(appVersion());
    rmSync(prefsFile, { force: true });
    const before = await checks();
    await browser.reloadSession();

    const card = $(FIRST_RUN);
    await card.waitForDisplayed();
    await waitForText(FIRST_RUN, "Check for updates automatically?");
    expect(await checks()).toBe(before);

    await card.$("button=Yes, check").click();
    await waitForGone(FIRST_RUN);
    await waitForChecks(before + 1);
    await waitForHandledChecks(1);
    // The feed offers this same version: nothing to show.
    expect(await $$(NOTICE).length).toBe(0);
    await openSettings();
    await expect($(AUTO_CHECK)).toBeSelected();
    await settingsButton().click();
    await waitForGone(AUTO_CHECK);
  });

  it("shows no notice for the same version, and Check for updates says you're up to date", async () => {
    offer(appVersion());
    const before = await checks();
    await browser.reloadSession();

    // Automatic checks are on now: one runs on start.
    await waitForChecks(before + 1);
    await waitForHandledChecks(1);
    await expect($(FIRST_RUN)).not.toBeExisting();
    expect(await $$(NOTICE).length).toBe(0);

    await openSettings();
    await waitForText('[data-testid="app-version"]', `rdm ${appVersion()}`);
    await $('[data-testid="update-settings"]').$("button=Check for updates").click();
    await waitForText(RESULT, "You're up to date.");
    expect(await checks()).toBe(before + 2);
    await settingsButton().click();
    await waitForGone(AUTO_CHECK);
  });

  it("offers a newer version; Restart now pauses a running download for the update", async () => {
    const newer = newerVersion();
    offer(newer);
    await browser.reloadSession();

    const notice = $(NOTICE);
    await notice.waitForDisplayed();
    await waitForText(NOTICE, `rdm ${newer} is available`);
    await expect(notice.$("button=Install when I quit")).toBeDisplayed();
    await expect(notice.$("button=Later")).toBeDisplayed();

    // What's new opens the found release's page, logged instead of opened.
    await notice.$("button=What's new").click();
    await browser.waitUntil(
      () => openLog().some((l) => l.startsWith("url ") && l.endsWith(`/releases/tag/v${newer}`)),
      { timeoutMsg: "What's new never opened the release page" },
    );

    const id = await addThroughDialog(`${baseUrl}/crawl`);
    ids.push(id);
    await waitForStatus(id, "downloading");
    await waitForText(
      '[data-testid="update-line"]',
      "1 download will pause; you can resume it after the restart.",
    );

    await notice.$("button=Restart now").click();
    // The seam stops where the installer would run and restart the app.
    await waitForText(NOTICE, `Restarting into rdm ${newer}`);
    await waitForStatus(id, "paused");
    const row = await engineRow(id);
    expect(row?.status).toBe("paused");
    expect(row?.paused_for_update).toBe(true);
  });

  it("after the restart, offers to resume exactly the downloads the update paused", async () => {
    const id = ids[ids.length - 1];
    await browser.reloadSession();

    const bar = $(PAUSED_BAR);
    await bar.waitForDisplayed();
    await waitForText(PAUSED_BAR, "1 download was paused for the update.");
    await waitForStatus(id, "paused");
    // The start-up check's notice mounts above the bar: let it land first,
    // so the button doesn't move mid-click.
    await waitForHandledChecks(1);

    await bar.$("button=Resume all").click();
    await waitForGone(PAUSED_BAR);
    await browser.waitUntil(
      async () => {
        const d = await engineRow(id);
        return d?.status !== "paused" && d?.paused_for_update === false;
      },
      { timeoutMsg: `download ${id} was never resumed` },
    );
  });

  it("after another restart, its dismiss leaves the downloads paused", async () => {
    const id = ids[ids.length - 1];
    await waitForStatus(id, "downloading");
    const notice = $(NOTICE);
    await waitForText(NOTICE, `rdm ${newerVersion()} is available`);
    await notice.$("button=Restart now").click();
    await waitForText(NOTICE, "Restarting into rdm");
    await waitForStatus(id, "paused");
    await browser.reloadSession();

    const bar = $(PAUSED_BAR);
    await bar.waitForDisplayed();
    await waitForHandledChecks(1);
    await bar.$('button[aria-label="Dismiss"]').click();
    await waitForGone(PAUSED_BAR);
    await browser.waitUntil(async () => (await engineRow(id))?.paused_for_update === false, {
      timeoutMsg: `download ${id} kept its paused-for-the-update mark`,
    });
    expect((await engineRow(id))?.status).toBe("paused");
  });
});
