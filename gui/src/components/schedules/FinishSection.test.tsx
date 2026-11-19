// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ActionsStatus, PostActions } from "../../lib/types";
import { CommandWarning } from "../table/CommandWarning";
import { FinishSection } from "./FinishSection";
import { PowerNotice } from "./PowerNotice";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const OFF: PostActions = {
  command: { enabled: false, program: "", args: [] },
  power: { enabled: false, action: "shutdown" },
};

const status = (power: ActionsStatus["power"]): ActionsStatus => ({ power });

describe("When downloads finish", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    document.body.querySelectorAll("[role=dialog]").forEach((d) => d.remove());
  });

  const render = (actions: PostActions, s: ActionsStatus | null = null) => {
    const save = vi.fn().mockResolvedValue(undefined);
    act(() =>
      root.render(<FinishSection actions={actions} status={s} save={save} onError={vi.fn()} />),
    );
    return save;
  };
  const q = (id: string) => host.querySelector<HTMLButtonElement>(`[data-testid="${id}"]`)!;
  const click = (el: Element) => act(() => (el as HTMLElement).click());

  it("shows two one-line rows with fixed titles, off by default, on the rules' switch column", () => {
    render(OFF);
    const rows = host.querySelectorAll("li");
    expect(rows).toHaveLength(2);
    expect(q("finish-command-title").textContent).toBe("After each download");
    expect(q("finish-power-title").textContent).toBe("After all downloads");
    expect(q("finish-command-switch").getAttribute("aria-checked")).toBe("false");
    expect(q("finish-power-switch").getAttribute("aria-checked")).toBe("false");
    expect(q("finish-command-switch").getAttribute("aria-label")).toBe("After each download on");
    expect(q("finish-power-switch").getAttribute("aria-label")).toBe("After all downloads on");
    // Title, switch and a spacer where a rule has its menu; no menu of their own.
    expect(rows[0].className).toContain("grid-cols-[1fr_auto_1.5rem]");
    expect(host.querySelector('[aria-label$="options"]')).toBeNull();
  });

  it("switches the power action on and off, keeping what it does", () => {
    const save = render({ ...OFF, power: { enabled: false, action: "sleep" } });
    expect(q("finish-power-title").textContent).toBe("After all downloads");
    expect(q("finish-power-chip").textContent).toBe("Sleep");
    click(q("finish-power-switch"));
    expect(save).toHaveBeenCalledWith({ ...OFF, power: { enabled: true, action: "sleep" } });
  });

  it("picks shut down, sleep or hibernate from the chip, with tooltips", () => {
    const save = render(OFF);
    const chip = q("finish-power-chip");
    expect(chip.textContent).toBe("Shut down");
    expect(chip.getAttribute("aria-haspopup")).toBe("menu");
    expect(chip.getAttribute("aria-expanded")).toBe("false");
    click(chip);
    expect(chip.getAttribute("aria-expanded")).toBe("true");
    const items = [...host.querySelectorAll<HTMLElement>('[role="menuitemradio"]')];
    expect(items.map((i) => i.textContent)).toEqual(["Shut down", "Sleep", "Hibernate"]);
    expect(items.map((i) => i.getAttribute("aria-checked"))).toEqual(["true", "false", "false"]);
    expect(items.map((i) => i.getAttribute("title"))).toEqual([
      null,
      "Low power; wakes in seconds",
      "Saves to disk and powers off completely",
    ]);
    click(items[2]);
    expect(save).toHaveBeenCalledWith({ ...OFF, power: { enabled: false, action: "hibernate" } });
  });

  it("closes the chip's list with Escape and moves between choices with the arrows", () => {
    render(OFF);
    click(q("finish-power-chip"));
    const items = [...host.querySelectorAll<HTMLElement>('[role="menuitemradio"]')];
    expect(document.activeElement).toBe(items[0]);
    act(() => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    });
    expect(document.activeElement).toBe(items[1]);
    act(() => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(host.querySelector('[role="menu"]')).toBeNull();
    expect(q("finish-power-chip").getAttribute("aria-expanded")).toBe("false");
  });

  it("says what a switched-on power action is waiting for", () => {
    render(
      { ...OFF, power: { enabled: true, action: "shutdown" } },
      status({ state: "waiting", reason: { kind: "command" } }),
    );
    expect(q("finish-power-note").textContent).toBe("Waiting for the command to finish");
  });

  it("reads No command, dimmed and without a tooltip, until a program is set", () => {
    render(OFF);
    const chip = q("finish-command-chip");
    expect(chip.textContent).toBe("No command");
    expect(chip.className).toContain("text-g400");
    expect(chip.hasAttribute("title")).toBe(false);
    click(chip);
    expect(document.querySelector('[role="dialog"]')!.getAttribute("aria-label")).toBe(
      "Edit command",
    );
  });

  it("shows a failed power action in amber under its title", () => {
    render(
      { ...OFF, power: { enabled: true, action: "sleep" } },
      status({ state: "failed", message: "denied" }),
    );
    expect(q("finish-power-note").textContent).toBe("Couldn't sleep: denied");
    expect(q("finish-power-note").className).toContain("text-warn-ink");
  });

  it("opens Edit command to switch on a command that has none yet", () => {
    const save = render(OFF);
    click(q("finish-command-switch"));
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.getAttribute("aria-label")).toBe("Edit command");
    expect(dialog.textContent).toContain("Runs with your rights");
    // Nothing to save without a program.
    expect(
      dialog.querySelector<HTMLButtonElement>('[data-testid="command-submit"]')!.disabled,
    ).toBe(true);
    expect(save).not.toHaveBeenCalled();
  });

  it("keeps the empty program hint neutral until it is touched", () => {
    render(OFF);
    click(q("finish-command-switch"));
    const hint = document.querySelector<HTMLElement>('[data-testid="command-problem"]')!;
    expect(hint.textContent).toBe("");
    expect(hint.className).not.toContain("danger");
    const input = document.querySelector<HTMLInputElement>("#cmd-program")!;
    act(() => {
      input.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    });
    expect(hint.textContent).toBe("Enter the program to run");
    expect(hint.className).toContain("danger");
  });

  it("saves the program and one argument per line", async () => {
    const save = render({
      ...OFF,
      command: { enabled: false, program: "scan.exe", args: ["{path}"] },
    });
    expect(q("finish-command-chip").textContent).toBe("scan.exe {path}");
    expect(q("finish-command-chip").getAttribute("title")).toBe("scan.exe {path}");
    click(q("finish-command-chip"));
    const textarea = document.querySelector<HTMLTextAreaElement>("textarea")!;
    const set = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!;
    act(() => {
      set.call(textarea, "--name\n{name} a;b\n\n{path}");
      textarea.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () =>
      document.querySelector<HTMLButtonElement>('[data-testid="command-submit"]')!.click(),
    );
    // Editing keeps the command's switch where it was.
    expect(save).toHaveBeenCalledWith({
      ...OFF,
      command: { enabled: false, program: "scan.exe", args: ["--name", "{name} a;b", "{path}"] },
    });
  });
});

describe("the countdown notice and the command warning", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("appears only during a countdown, and Cancel calls back", () => {
    const onCancel = vi.fn();
    act(() => root.render(<PowerNotice status={status({ state: "ready" })} onCancel={onCancel} />));
    expect(host.textContent).toBe("");

    act(() =>
      root.render(
        <PowerNotice
          status={status({ state: "countdown", seconds_left: 42, action: "sleep" })}
          onCancel={onCancel}
        />,
      ),
    );
    expect(host.textContent).toContain("Going to sleep in 42 s");
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="power-cancel"]')!.click());
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("flags a failed command with an icon only, the reason as its tooltip and label", () => {
    act(() => root.render(<CommandWarning message="scan.exe exited with code 3" />));
    expect(host.textContent).toBe("");
    expect(host.querySelector("svg")).not.toBeNull();
    const flag = host.querySelector("[title]")!;
    expect(flag.getAttribute("title")).toBe("Command failed: scan.exe exited with code 3");
    expect(flag.getAttribute("aria-label")).toBe("Command failed: scan.exe exited with code 3");
    expect(flag.getAttribute("role")).toBe("img");
  });
});
