// End-to-end: "When downloads finish" in the Schedules view. Two one-line rows, off by default, on
// the same switch column as the rules, each a fixed title and a chip. The power chip picks shut
// down, sleep or hibernate; the command chip opens Edit command (a program and one argument per
// line, with the note that it runs with your rights), as does switching the row on with no
// command yet. A finished download whose command can't start gets a warning on its row and stays
// completed. A power action counts down in a notice on every page, and Cancel stops it. The app's
// power backend is the e2e build's log file (a build that cannot reach the real machine), so
// nothing here can sleep or shut anything down.
// The actions live in the shared app state, so the spec resets them before and after.

import { existsSync } from "node:fs";
import { $, browser, expect } from "@wdio/globals";
import {
  addThroughDialog,
  baseUrl,
  invoke,
  Key,
  rectOf,
  removeAll,
  retype,
  selectFilter,
  waitForGone,
  waitForStatus,
  waitForText,
} from "../helpers";

const SCHEDULES_LINK = '[data-testid="schedules-link"]';
const VIEW = '[data-testid="schedules-view"]';
const COMMAND_ROW = '[data-testid="finish-command"]';
const POWER_ROW = '[data-testid="finish-power"]';
const COMMAND_CHIP = '[data-testid="finish-command-chip"]';
const POWER_CHIP = '[data-testid="finish-power-chip"]';
const POWER_MENU = '[data-testid="finish-power-menu"]';
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

/** Reloads the page and comes back to Schedules, to see what a restart would show. */
async function reopen(): Promise<void> {
  await browser.refresh();
  await $(SCHEDULES_LINK).click();
  await $(VIEW).waitForDisplayed();
}

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

  it("lists both actions, off, with fixed titles and chips that start together", async () => {
    await waitForText('[data-testid="finish-command-title"]', "After each download");
    await waitForText('[data-testid="finish-power-title"]', "After all downloads");
    await waitForText(COMMAND_CHIP, "No command");
    await waitForText(POWER_CHIP, "Shut down");
    await expect($(`${COMMAND_ROW} [role="switch"]`)).toHaveAttribute("aria-checked", "false");
    await expect($(`${POWER_ROW} [role="switch"]`)).toHaveAttribute("aria-checked", "false");
    await expect($(`${COMMAND_ROW} [role="switch"]`)).toHaveAttribute(
      "aria-label",
      "After each download on",
    );
    await expect($(`${POWER_ROW} [role="switch"]`)).toHaveAttribute(
      "aria-label",
      "After all downloads on",
    );
    expect((await rectOf(COMMAND_CHIP)).x).toBeCloseTo((await rectOf(POWER_CHIP)).x, 0);
  });

  it("picks what the power action does from its chip, keeps it across a refresh, and switches it on and off", async () => {
    await expect($(POWER_CHIP)).toHaveAttribute("aria-haspopup", "menu");
    await $(POWER_CHIP).click();
    await expect($(POWER_CHIP)).toHaveAttribute("aria-expanded", "true");
    const choices = await $(POWER_MENU).$$("button");
    expect(await Promise.all(choices.map((c) => c.getText()))).toEqual([
      "Sleep",
      "Shut down",
      "Hibernate",
    ]);
    await expect($(POWER_MENU).$("button=Sleep")).toHaveAttribute(
      "title",
      "Low power; wakes in seconds",
    );
    await expect($(POWER_MENU).$("button=Hibernate")).toHaveAttribute(
      "title",
      "Saves to disk and powers off completely",
    );
    await $(POWER_MENU).$("button=Hibernate").click();
    await waitForText(POWER_CHIP, "Hibernate");
    await waitForText('[data-testid="finish-power-title"]', "After all downloads");
    expect((await saved()).power).toEqual({ enabled: false, action: "hibernate" });

    await reopen();
    await waitForText(POWER_CHIP, "Hibernate");

    await $(POWER_CHIP).click();
    await $(POWER_MENU).$("button=Sleep").click();
    await waitForText(POWER_CHIP, "Sleep");
    expect((await saved()).power).toEqual({ enabled: false, action: "sleep" });

    await $(`${POWER_ROW} [role="switch"]`).click();
    await expect($(`${POWER_ROW} [role="switch"]`)).toHaveAttribute("aria-checked", "true");
    expect((await saved()).power).toEqual({ enabled: true, action: "sleep" });

    await $(`${POWER_ROW} [role="switch"]`).click();
    await expect($(`${POWER_ROW} [role="switch"]`)).toHaveAttribute("aria-checked", "false");
    expect((await saved()).power.enabled).toBe(false);
  });

  it("lines the finish switches up with a rule's switch, one line each", async () => {
    const days = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
    // Switched off, so it holds no download back in the later tests.
    await invoke("set_schedule", {
      schedule: {
        queue_windows: [
          { enabled: false, window: { start: "01:00", end: "02:00", days }, finish_running: false },
        ],
        speed_entries: [],
      },
    });
    try {
      await reopen();
      await $('[data-testid="rule-switch"]').waitForDisplayed();
      const rule = await rectOf('[data-testid="rule-switch"]');
      expect((await rectOf(`${COMMAND_ROW} [role="switch"]`)).x).toBeCloseTo(rule.x, 0);
      expect((await rectOf(`${POWER_ROW} [role="switch"]`)).x).toBeCloseTo(rule.x, 0);
      expect((await rectOf(POWER_ROW)).height).toBeLessThan(48);
      await expect($(`${POWER_ROW} button[aria-label$="options"]`)).not.toExist();
      await expect($(`${COMMAND_ROW} button[aria-label$="options"]`)).not.toExist();
    } finally {
      await invoke("set_schedule", { schedule: { queue_windows: [], speed_entries: [] } });
      await reopen();
    }
  });

  it("opens Edit command from the chip, and Cancel leaves the row off", async () => {
    await $(COMMAND_CHIP).click();
    await $(FORM).waitForDisplayed();
    await $("button=Cancel").click();
    await waitForGone(FORM);
    await expect($(`${COMMAND_ROW} [role="switch"]`)).toHaveAttribute("aria-checked", "false");
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
    await args.addValue("{download_path}");
    await $('[data-testid="command-submit"]').click();
    await waitForGone(FORM);

    await expect($(`${COMMAND_ROW} [role="switch"]`)).toHaveAttribute("aria-checked", "true");
    await waitForText(COMMAND_CHIP, "rdm-e2e-no-such-program --scan {download_path}");
    await expect($(COMMAND_CHIP)).toHaveAttribute(
      "title",
      "rdm-e2e-no-such-program --scan {download_path}",
    );
    expect((await saved()).command).toEqual({
      enabled: true,
      program: "rdm-e2e-no-such-program",
      args: ["--scan", "{download_path}"],
    });
    // The variable is its own token on the chip, with its explanation.
    const tokens = await browser.execute(
      (sel) =>
        [...document.querySelector(sel)!.querySelectorAll("span[title]")].map((t) => [
          t.textContent,
          t.getAttribute("title"),
        ]),
      COMMAND_CHIP,
    );
    expect(tokens).toEqual([["{download_path}", expect.stringContaining("with its folder")]]);
  });

  it("inserts a variable into Arguments from the Insert buttons, even from the Program field", async () => {
    await $(COMMAND_CHIP).click();
    await $(FORM).waitForDisplayed();
    const args = $('textarea[aria-label="Arguments"]');
    // Focus is on Program; the buttons still fill Arguments, after the existing text.
    await $('input[aria-label="Program"]').click();
    await $('[data-testid="insert-download_name"]').click();
    await expect(args).toHaveValue(expect.stringContaining("{download_name}"));
    await expect($('[data-testid="insert-download_name"]')).toHaveAttribute(
      "title",
      "Just the file's name. Example: report.zip",
    );
    expect(await browser.execute(() => document.activeElement?.getAttribute("aria-label"))).toBe(
      "Arguments",
    );
    await $('[data-testid="command-submit"]').click();
    await waitForGone(FORM);
    await waitForText(COMMAND_CHIP, "{download_name}");
    const saved1 = (await saved()).command.args;
    expect(saved1.filter((a: string) => a.includes("{download_name}"))).toHaveLength(1);
    expect(saved1).toContain("{download_path}");

    // Put the failing command the later tests rely on back.
    await invoke("set_post_actions", {
      actions: {
        command: {
          enabled: true,
          program: "rdm-e2e-no-such-program",
          args: ["--scan", "{download_path}"],
        },
        power: { enabled: false, action: "sleep" },
      },
    });
    await reopen();
  });

  it("cuts a long command with an ellipsis at the end and keeps the whole line as its tooltip", async () => {
    const program = `C:\\Tools\\${"scan-".repeat(30)}viewer.exe`;
    await invoke("set_post_actions", {
      actions: { ...OFF, command: { enabled: true, program, args: ["{download_path}"] } },
    });
    await reopen();
    await waitForText(COMMAND_CHIP, "C:\\Tools\\scan-");
    expect(await $(COMMAND_CHIP).getAttribute("title")).toBe(`${program} {download_path}`);
    // The chip stops at about 26rem (416px at the default size) instead of growing with the text.
    expect((await rectOf(COMMAND_CHIP)).width).toBeLessThanOrEqual(420);
    const cut = await browser.execute((sel) => {
      const span = document.querySelector(sel)!.querySelector("span")!;
      return span.scrollWidth > span.clientWidth;
    }, COMMAND_CHIP);
    expect(cut).toBe(true);

    // Back to the failing command the later tests rely on.
    await invoke("set_post_actions", {
      actions: {
        command: {
          enabled: true,
          program: "rdm-e2e-no-such-program",
          args: ["--scan", "{download_path}"],
        },
        power: { enabled: false, action: "sleep" },
      },
    });
    await reopen();
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
    const warning = `tr[data-id="${id}"] td.c-prog [data-testid="command-warning"]`;
    await $(warning).waitForExist({ timeoutMsg: "the failed command never showed on its row" });
    // An icon in the Progress cell, with the reason as its label and tooltip; the name keeps its room.
    expect(await $(warning).getAttribute("aria-label")).toMatch(/^Command failed: /);
    expect(await $(warning).getAttribute("title")).toMatch(/^Command failed: /);
    await expect($(`tr[data-id="${id}"] td.c-name [data-testid="command-warning"]`)).not.toExist();
    const name = await rectOf(`tr[data-id="${id}"] td.c-name`);
    expect((await rectOf(warning)).x).toBeGreaterThanOrEqual(name.x + name.width);
    const row = (await invoke<{ id: number; status: string; error: string | null }[]>("list")).find(
      (d) => d.id === id,
    )!;
    expect(row.status).toBe("completed");
    expect(row.error).toBeNull();
  });
});
