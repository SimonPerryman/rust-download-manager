// End-to-end: the Proxy setting. A proxy set in the Settings popover routes
// the next download through the fixture's test proxy, and the field shows it
// back with the password masked, as Copy diagnostics leaves its login out; an
// invalid URL is refused in place (or, left by closing the popover, in a
// toast) and changes nothing. Shares the app state dir with the other specs (see
// wdio.conf.ts), so it clears the proxy and removes its downloads when done.

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

const PROXY_INPUT = 'input[aria-label="Proxy"]';
const MENU = '[role="dialog"][aria-label="Settings"]';
const proxyUrl = process.env.RDM_E2E_PROXY_URL!;
// The fixture proxy takes any login; this one is only there to be masked.
const withLogin = proxyUrl.replace("://", "://e2e:hunter2@");

/** How many requests the fixture's test proxy has relayed. */
async function proxied(): Promise<number> {
  return Number(await (await fetch(`${proxyUrl}/seen`)).text());
}

/** Opens the Settings popover and returns the Proxy field, once loaded. */
async function openProxyField(): Promise<ReturnType<typeof $>> {
  await settingsButton().click();
  const input = $(PROXY_INPUT);
  await expect(input).toBeEnabled();
  return input;
}

async function closeSettings(): Promise<void> {
  await settingsButton().click();
  await waitForGone(PROXY_INPUT);
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
      await invoke("set_settings", { proxy: "" });
      expect((await invoke<{ proxy: string | null }>("get_settings")).proxy).toBeNull();
    } finally {
      await removeAll(ids);
    }
  });

  it("downloads through the proxy set in Settings, and masks its password", async () => {
    await retype(await openProxyField(), withLogin);
    await closeSettings();

    // Reopening re-reads the setting from the engine: the password is masked.
    const masked = withLogin.replace("hunter2", "••••••");
    await expect(await openProxyField()).toHaveValue(masked);
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
    await openProxyField();
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
    const input = await openProxyField();
    await retype(input, "not a proxy");
    await expect(input).toHaveAttribute("aria-invalid", "true");
    await expect($('[data-testid="download-settings"]')).toHaveText("Invalid proxy", {
      containing: true,
    });
    await closeSettings();
    // Closing with the bad edit still in the field says so again, as a toast.
    await $(TOAST).waitForExist();
    await dismissOverlays();

    const saved = await invoke<{ proxy: string | null }>("get_settings");
    expect(saved.proxy).toBe(withLogin.replace("hunter2", "••••••"));
  });

  it("says so in a toast when an invalid proxy is left by closing the popover", async () => {
    await retype(await openProxyField(), "not a proxy", { enter: false });
    await closeSettings();

    await expect($(TOAST)).toHaveText("Proxy not saved", { containing: true });
    const saved = await invoke<{ proxy: string | null }>("get_settings");
    expect(saved.proxy).toBe(withLogin.replace("hunter2", "••••••"));
    await dismissOverlays();
  });
});
