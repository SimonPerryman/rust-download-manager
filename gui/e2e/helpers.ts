// Shared e2e helpers.

import { $$, browser } from "@wdio/globals";

/** Waits until nothing matches `selector`. */
export async function waitForGone(selector: string, timeoutMsg?: string): Promise<void> {
  await browser.waitUntil(
    async () => {
      // A fresh query each poll, and a thrown error means "not gone yet":
      // WebKitGTK's driver reports an element detached mid-check as a stale
      // element, not as absent, so waitForExist({ reverse: true }) flakes.
      try {
        return (await $$(selector).length) === 0;
      } catch {
        return false;
      }
    },
    { timeoutMsg: timeoutMsg ?? `${selector} never went away` },
  );
}
