// End-to-end: the app still renders under its strict content security policy
// (tauri.conf.json `app.security.csp`). Tauri hashes the inline theme script
// into `script-src` at build time, so it must still run; the bundled IBM Plex
// fonts must load from 'self'; and adding a download (which goes over IPC)
// must not trip any CSP violation.

import { $, browser, expect } from "@wdio/globals";
import { ADD_DIALOG, addThroughDialog, baseUrl, removeAll, waitForStatus } from "../helpers";

interface Probe {
  violations: string[];
}

describe("rdm GUI content security policy", () => {
  const ids: number[] = [];

  before(async () => {
    // Violations from here on are collected; the page's own load is checked by its effects below.
    await browser.execute(() => {
      const w = window as unknown as { __cspProbe: Probe };
      w.__cspProbe = { violations: [] };
      document.addEventListener("securitypolicyviolation", (e) => {
        w.__cspProbe.violations.push(`${e.violatedDirective} ${e.blockedURI}`);
      });
    });
  });

  after(async () => {
    await removeAll(ids);
  });

  it("runs the inline theme script before first paint", async () => {
    const theme = await browser.execute(() => document.documentElement.dataset.theme ?? null);
    expect(["system", "light", "dark"]).toContain(theme);
  });

  it("loads the bundled fonts", async () => {
    const faces = await browser.execute(async () => {
      await document.fonts.ready;
      return [...document.fonts]
        .filter((f) => f.status === "loaded")
        .map((f) => f.family.replace(/["']/g, ""));
    });
    expect(faces).toContain("IBM Plex Sans");
    const family = await browser.execute(() => getComputedStyle(document.body).fontFamily);
    expect(family).toContain("IBM Plex Sans");
  });

  it("adds and finishes a download with no CSP violation", async () => {
    const id = await addThroughDialog(`${baseUrl}/file`);
    ids.push(id);
    await waitForStatus(id, "completed");
    expect(await $(ADD_DIALOG).isExisting()).toBe(false);
    const violations = await browser.execute(
      () => (window as unknown as { __cspProbe: Probe }).__cspProbe.violations,
    );
    expect(violations).toEqual([]);
  });

  // Proves the policy is actually enforced, so the checks above aren't passing on no CSP at all.
  it("blocks an injected inline script", async () => {
    await browser.execute(() => {
      const s = document.createElement("script");
      s.textContent = "window.__cspInlineRan = true;";
      document.head.appendChild(s);
    });
    await browser.waitUntil(
      () =>
        browser.execute(() =>
          (window as unknown as { __cspProbe: Probe }).__cspProbe.violations.some((v) =>
            v.startsWith("script-src"),
          ),
        ),
      { timeoutMsg: "no script-src violation reported for an injected inline script" },
    );
    const ran = await browser.execute(
      () => (window as unknown as { __cspInlineRan?: boolean }).__cspInlineRan ?? false,
    );
    expect(ran).toBe(false);
  });
});
