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

  it("shows two rows, off by default, on the rules' switch column", () => {
    render(OFF);
    const rows = host.querySelectorAll("li");
    expect(rows).toHaveLength(2);
    expect(rows[0].textContent).toContain("Run a command after each download");
    expect(rows[1].textContent).toContain("Shut down when everything is done");
    expect(q("finish-command-switch").getAttribute("aria-checked")).toBe("false");
    expect(q("finish-power-switch").getAttribute("aria-checked")).toBe("false");
    // The same three columns as a rule row.
    expect(rows[0].className).toContain("grid-cols-[1fr_auto_auto]");
  });

  it("switches the power action on and off, keeping what it does", () => {
    const save = render({ ...OFF, power: { enabled: false, action: "sleep" } });
    expect(host.textContent).toContain("Sleep when everything is done");
    click(q("finish-power-switch"));
    expect(save).toHaveBeenCalledWith({ ...OFF, power: { enabled: true, action: "sleep" } });
  });

  it("picks shut down, sleep or hibernate from the row menu", () => {
    const save = render(OFF);
    click(q("finish-power-menu-button"));
    const items = [...host.querySelectorAll<HTMLElement>('[role="menuitemradio"]')];
    expect(items.map((i) => i.textContent)).toEqual(["Shut down", "Sleep", "Hibernate"]);
    expect(items.map((i) => i.getAttribute("aria-checked"))).toEqual(["true", "false", "false"]);
    click(items[2]);
    expect(save).toHaveBeenCalledWith({ ...OFF, power: { enabled: false, action: "hibernate" } });
  });

  it("says what a switched-on power action is waiting for", () => {
    render(
      { ...OFF, power: { enabled: true, action: "shutdown" } },
      status({ state: "waiting", reason: { kind: "command" } }),
    );
    expect(q("finish-power-note").textContent).toBe("Waiting for the command to finish");
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

  it("saves the program and one argument per line", async () => {
    const save = render({
      ...OFF,
      command: { enabled: false, program: "scan.exe", args: ["{path}"] },
    });
    expect(q("finish-command-note").textContent).toBe("scan.exe {path}");
    click(q("finish-command-menu-button"));
    click([...host.querySelectorAll<HTMLElement>('[role="menuitem"]')][0]);
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

  it("flags a failed command with an icon and words, and the reason as its tooltip", () => {
    act(() => root.render(<CommandWarning message="scan.exe exited with code 3" />));
    expect(host.textContent).toBe("Command failed");
    expect(host.querySelector("svg")).not.toBeNull();
    expect(host.querySelector("[title]")!.getAttribute("title")).toBe(
      "scan.exe exited with code 3",
    );
  });
});
