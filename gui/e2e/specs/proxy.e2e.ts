// End-to-end: the Proxy setting. It starts on Use system proxy; a custom
// proxy set in the Settings popover routes the next download through the
// fixture's test proxy, and the URL field shows it back with the password
// masked, as Copy diagnostics leaves its login out; an invalid URL is refused
// in place (or, left by closing the popover, in a toast) and changes nothing;
// No proxy goes direct. Shares the app state dir with the other specs (see
// wdio.conf.ts), so it puts the system proxy back and removes its downloads
// when done.

import { $, browser, expect } from "@wdio/globals";
import {
  addThroughDialog,
  baseUrl,
  clipboardWrites,
  dismissOverlays,
  invoke,
  rectOf,
  removeAll,
  restoreClipboard,
  retype,
  selectFilter,
  settingsButton,
  stubClipboard,
  TOAST,
  waitForGone,
  waitForStatus,
} from "../helpers";

const PROXY_MODE = 'select[aria-label="Proxy"]';
const PROXY_URL = 'input[aria-label="Proxy URL"]';
const MENU = '[role="dialog"][aria-label="Settings"]';
const proxyUrl = process.env.RDM_E2E_PROXY_URL!;
// The fixture proxy takes any login; this one is only there to be masked.
const withLogin = proxyUrl.replace("://", "://e2e:hunter2@");
const masked = withLogin.replace("hunter2", "••••••");

type ProxySetting = { mode: string; url?: string };

/** How many requests the fixture's test proxy has relayed. */
async function proxied(): Promise<number> {
  return Number(await (await fetch(`${proxyUrl}/seen`)).text());
}

/** The proxy setting as the engine has it. */
async function savedProxy(): Promise<ProxySetting> {
  return (await invoke<{ proxy: ProxySetting }>("get_settings")).proxy;
}

/** Opens the Settings popover and returns the Proxy choice, once loaded. */
async function openProxySetting(): Promise<ReturnType<typeof $>> {
  await settingsButton().click();
  const select = $(PROXY_MODE);
  await expect(select).toBeEnabled();
  return select;
}

async function closeSettings(): Promise<void> {
  await settingsButton().click();
  await waitForGone(PROXY_MODE);
}

describe("rdm GUI proxy setting", () => {
  const ids: number[] = [];

  before(async () => {
    await selectFilter("all");
    await stubClipboard();
  });

  after(async () => {
    try {
      await restoreClipboard();
      if ((await settingsButton().getAttribute("aria-expanded")) === "true") {
        await closeSettings();
      }
      // Straight to the engine, so no later spec downloads through the proxy.
      await invoke("set_settings", { proxy: { mode: "system" } });
      expect(await savedProxy()).toEqual({ mode: "system" });
    } finally {
      await removeAll(ids);
    }
  });

  it("starts on Use system proxy, with no URL field", async () => {
    expect(await savedProxy()).toEqual({ mode: "system" });
    await expect(await openProxySetting()).toHaveValue("system");
    await expect($(PROXY_URL)).not.toExist();
    await closeSettings();
  });

  it("downloads through a custom proxy set in Settings, and masks its password", async () => {
    await (await openProxySetting()).selectByAttribute("value", "custom");
    const url = $(PROXY_URL);
    await url.waitForExist();
    // Picking Custom alone saves nothing: it waits for the URL.
    expect(await savedProxy()).toEqual({ mode: "system" });
    await retype(url, withLogin);
    await closeSettings();
    expect(await savedProxy()).toEqual({ mode: "custom", url: masked });

    // Reopening re-reads the setting from the engine: the password is masked.
    await expect(await openProxySetting()).toHaveValue("custom");
    await expect($(PROXY_URL)).toHaveValue(masked);
    await closeSettings();

    const before = await proxied();
    const id = await addThroughDialog(`${baseUrl}/file`);
    ids.push(id);
    await waitForStatus(id, "completed");
    expect(await proxied()).toBeGreaterThan(before);
  });

  it("copies diagnostics without the proxy's login, from a popover that fits the window", async () => {
    // A finish notification sits above the popover and could cover its buttons.
    await dismissOverlays();
    await openProxySetting();
    const menu = await rectOf(MENU);
    const windowHeight = await browser.execute(() => window.innerHeight);
    expect(menu.y + menu.height).toBeLessThanOrEqual(windowHeight);

    const copied = (await clipboardWrites()).length;
    await $(`${MENU} [data-testid="copy-diagnostics"]`).click();
    await browser.waitUntil(async () => (await clipboardWrites()).length > copied, {
      timeoutMsg: "Copy diagnostics never wrote to the clipboard",
    });
    const report = (await clipboardWrites()).at(-1)!;
    expect(report).toContain("proxy http ");
    expect(report).not.toContain("hunter2");
    expect(report).not.toContain("e2e:");
    await closeSettings();
  });

  it("refuses an invalid proxy in place and keeps the saved one", async () => {
    await openProxySetting();
    const url = $(PROXY_URL);
    await retype(url, "not a proxy");
    await expect(url).toHaveAttribute("aria-invalid", "true");
    await expect($('[data-testid="download-settings"]')).toHaveText("Invalid proxy", {
      containing: true,
    });
    await closeSettings();
    // Closing with the bad edit still in the field says so again, as a toast.
    await $(TOAST).waitForExist();
    await dismissOverlays();

    expect(await savedProxy()).toEqual({ mode: "custom", url: masked });
  });

  it("says so in a toast when an invalid proxy is left by closing the popover", async () => {
    await openProxySetting();
    await retype($(PROXY_URL), "not a proxy", { enter: false });
    await closeSettings();

    await expect($(TOAST)).toHaveText("Proxy not saved", { containing: true });
    expect(await savedProxy()).toEqual({ mode: "custom", url: masked });
    await dismissOverlays();
  });

  it("goes direct with No proxy, and back to the system proxy", async () => {
    await (await openProxySetting()).selectByAttribute("value", "none");
    await waitForGone(PROXY_URL);
    await browser.waitUntil(async () => (await savedProxy()).mode === "none", {
      timeoutMsg: "No proxy was never saved",
    });
    await closeSettings();

    const before = await proxied();
    const id = await addThroughDialog(`${baseUrl}/file`);
    ids.push(id);
    await waitForStatus(id, "completed");
    expect(await proxied()).toBe(before);

    await dismissOverlays();
    await (await openProxySetting()).selectByAttribute("value", "system");
    await browser.waitUntil(async () => (await savedProxy()).mode === "system", {
      timeoutMsg: "Use system proxy was never saved",
    });
    await closeSettings();
  });
});
