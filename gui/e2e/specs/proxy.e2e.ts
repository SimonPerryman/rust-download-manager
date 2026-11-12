// End-to-end: the Proxy setting. A proxy set in the Settings popover routes
// the next download through the fixture's test proxy, and the field shows it
// back with the password masked; an invalid URL is refused in place and
// changes nothing. Shares the app state dir with the other specs (see
// wdio.conf.ts), so it clears the proxy and removes its downloads when done.

import { $, expect } from "@wdio/globals";
import {
  addThroughDialog,
  baseUrl,
  invoke,
  removeAll,
  retype,
  selectFilter,
  settingsButton,
  waitForGone,
  waitForStatus,
} from "../helpers";

const PROXY_INPUT = 'input[aria-label="Proxy"]';
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
  });

  after(async () => {
    try {
      if ((await settingsButton().getAttribute("aria-expanded")) !== "true") {
        await settingsButton().click();
      }
      await retype($(PROXY_INPUT), "");
      await closeSettings();
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

  it("refuses an invalid proxy in place and keeps the saved one", async () => {
    const input = await openProxyField();
    await retype(input, "not a proxy");
    await expect(input).toHaveAttribute("aria-invalid", "true");
    await expect($('[data-testid="download-settings"]')).toHaveText("Invalid proxy", {
      containing: true,
    });
    await closeSettings();

    const saved = await invoke<{ proxy: string | null }>("get_settings");
    expect(saved.proxy).toBe(withLogin.replace("hunter2", "••••••"));
  });
});
