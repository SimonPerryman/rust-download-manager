// End-to-end: "When downloads finish" in the Schedules view. Two rows, off by default, on the same
// switch column as the rules. The power row's menu picks shut down, sleep or hibernate; the
// command row opens Edit command (a program and one argument per line, with the note that it runs
// with your rights) when switched on with no command yet. A finished download whose command can't
// start gets a warning on its row and stays completed. A power action counts down in a notice on
// every page, and Cancel stops it. The app's power backend is the e2e build's log file (a build
// that cannot reach the real machine), so nothing here can sleep or shut anything down.
// The actions live in the shared app state, so the spec resets them before and after.

import { existsSync } from "node:fs";
import { $, expect } from "@wdio/globals";
import {
  addThroughDialog,
  baseUrl,
  invoke,
  Key,
  removeAll,
  retype,
  rowById,
  selectFilter,
  waitForGone,
  waitForStatus,
  waitForText,
} from "../helpers";

const SCHEDULES_LINK = '[data-testid="schedules-link"]';
const VIEW = '[data-testid="schedules-view"]';
const COMMAND_ROW = '[data-testid="finish-command"]';
const POWER_ROW = '[data-testid="finish-power"]';
const FORM = '[role="dialog"][aria-label="Edit command"]';
const NOTICE = '[data-testid="power-notice"]';
const POWER_LOG = process.env.RDM_POWER_LOG!;

interface Saved {
  command: { enabled: boolean; program: string; args: string[] };
  power: { enabled: boolean; action: string };
}

const OFF: Saved = {
  command: { enabled: false, program: "", args: [] },
  power: { enabled: false, action: "shutdown" },
};

const saved = () => invoke<Saved>("get_post_actions");
const reset = () => invoke("set_post_actions", { actions: OFF });

describe("rdm GUI when downloads finish", () => {
  const ids: number[] = [];

  before(async () => {
    await reset();
    await $(SCHEDULES_LINK).click();
    await $(VIEW).waitForDisplayed();
  });

  after(async () => {
    await reset();
    await removeAll(ids);
    await selectFilter("all");
  });

  it("lists both actions, off, under When downloads finish", async () => {
    await waitForText('[data-testid="finish-actions"]', "Run a command after each download");
    await waitForText('[data-testid="finish-actions"]', "Shut down when everything is done");
    await expect($(`${COMMAND_ROW} [role="switch"]`)).toHaveAttribute("aria-checked", "false");
    await expect($(`${POWER_ROW} [role="switch"]`)).toHaveAttribute("aria-checked", "false");
  });

  it("picks what the power action does from its menu, and switches it on and off", async () => {
    await $('[data-testid="finish-power-menu-button"]').click();
    await $('[data-testid="finish-power-menu"]').$("button=Sleep").click();
    await waitForText(POWER_ROW, "Sleep when everything is done");
    expect((await saved()).power).toEqual({ enabled: false, action: "sleep" });

    await $(`${POWER_ROW} [role="switch"]`).click();
    await expect($(`${POWER_ROW} [role="switch"]`)).toHaveAttribute("aria-checked", "true");
    expect((await saved()).power).toEqual({ enabled: true, action: "sleep" });

    await $(`${POWER_ROW} [role="switch"]`).click();
    await expect($(`${POWER_ROW} [role="switch"]`)).toHaveAttribute("aria-checked", "false");
    expect((await saved()).power.enabled).toBe(false);
  });

  it("opens Edit command to switch on a command, which needs a program and says it runs as you", async () => {
    await $(`${COMMAND_ROW} [role="switch"]`).click();
    await $(FORM).waitForDisplayed();
    await waitForText(FORM, "Runs with your rights");
    await expect($('[data-testid="command-submit"]')).toBeDisabled();

    // A program that does not exist, so the run fails to start (a warning, never a failed download).
    await retype($('input[aria-label="Program"]'), "rdm-e2e-no-such-program", { enter: false });
    // One argument per line. The Enter is a key press: WebKitGTK's driver drops a "\n" sent as text.
    const args = $('textarea[aria-label="Arguments"]');
    await args.addValue("--scan");
    await browser.keys(Key.Enter);
    await args.addValue("{path}");
    await $('[data-testid="command-submit"]').click();
    await waitForGone(FORM);

    await expect($(`${COMMAND_ROW} [role="switch"]`)).toHaveAttribute("aria-checked", "true");
    await waitForText(COMMAND_ROW, "rdm-e2e-no-such-program --scan {path}");
    expect((await saved()).command).toEqual({
      enabled: true,
      program: "rdm-e2e-no-such-program",
      args: ["--scan", "{path}"],
    });
  });

  it("counts down once a download is done, on every page, and Cancel stops it", async () => {
    await $(`${POWER_ROW} [role="switch"]`).click();
    await expect($(`${POWER_ROW} [role="switch"]`)).toHaveAttribute("aria-checked", "true");

    await selectFilter("all");
    const id = await addThroughDialog(`${baseUrl}/file`);
    ids.push(id);
    await waitForStatus(id, "completed");

    await $(NOTICE).waitForDisplayed({ timeoutMsg: "the countdown never showed" });
    await waitForText(NOTICE, "Going to sleep in");
    // The notice follows the user to the Schedules view.
    await $(SCHEDULES_LINK).click();
    await $(VIEW).waitForDisplayed();
    await waitForText(NOTICE, "Going to sleep in");
    await $('[data-testid="power-cancel"]').click();
    await waitForGone(NOTICE);

    const status = await invoke<{ power: { state: string } }>("get_actions_status");
    expect(status.power.state).toBe("ready");
    expect(existsSync(POWER_LOG)).toBe(false);
  });

  it("flags the failed command on the download's row, which stays completed", async () => {
    await selectFilter("all");
    const id = ids[0];
    await rowById(id).$('[data-testid="command-warning"]').waitForExist({
      timeoutMsg: "the failed command never showed on its row",
    });
    await waitForText(`tr[data-id="${id}"]`, "Command failed");
    const row = (await invoke<{ id: number; status: string; error: string | null }[]>("list")).find(
      (d) => d.id === id,
    )!;
    expect(row.status).toBe("completed");
    expect(row.error).toBeNull();
  });
});
