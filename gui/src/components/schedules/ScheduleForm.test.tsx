// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NEW_WINDOW, ruleOf } from "../../lib/schedule";
import { RuleRow } from "./RuleRow";
import { ScheduleForm } from "./ScheduleForm";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("schedule components", () => {
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

  const q = <T extends Element>(sel: string) => host.querySelector<T>(sel)!;
  const click = (el: Element) =>
    act(() => {
      el.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
  const pick = (select: HTMLSelectElement, value: string) =>
    act(() => {
      const set = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")!.set!;
      set.call(select, value);
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });

  function form(onSubmit = vi.fn().mockResolvedValue(undefined)) {
    act(() =>
      root.render(
        <ScheduleForm initial={NEW_WINDOW} editing={false} onSubmit={onSubmit} onClose={vi.fn()} />,
      ),
    );
    return onSubmit;
  }

  it("says what the window does, and Add saves it", () => {
    const onSubmit = form();
    expect(q("[data-testid=schedule-summary]").textContent).toBe(
      "Downloads run from 22:00 to 08:00 every day, and pause at 08:00.",
    );
    const add = q<HTMLButtonElement>("[data-testid=schedule-submit]");
    expect(add.textContent).toBe("Add");
    click(add);
    expect(onSubmit).toHaveBeenCalledOnce();
  });

  it("changes the sentence when running downloads are left to finish", () => {
    form();
    click(q("[data-testid=finish-running]"));
    expect(q("[data-testid=schedule-summary]").textContent).toBe(
      "New downloads start from 22:00 to 08:00 every day. Running ones finish.",
    );
  });

  it("shows the day chips only under Choose days, and disables Add with none picked", () => {
    form();
    expect(host.querySelector("[data-testid=day-chips]")).toBeNull();
    pick(q("select[aria-label=Days]"), "choose");
    const chips = [...host.querySelectorAll<HTMLButtonElement>("[data-day]")];
    expect(chips.map((c) => c.textContent)).toEqual([
      "Mon",
      "Tue",
      "Wed",
      "Thu",
      "Fri",
      "Sat",
      "Sun",
    ]);
    // Starts from the days already picked: all seven.
    expect(chips.every((c) => c.getAttribute("aria-pressed") === "true")).toBe(true);
    chips.forEach(click);
    expect(q("[data-testid=schedule-summary]").textContent).toBe("Pick at least one day");
    expect(q<HTMLButtonElement>("[data-testid=schedule-submit]").disabled).toBe(true);
    click(chips[0]);
    click(chips[1]);
    click(chips[3]);
    expect(q("select[aria-label=Days] option[value=choose]").textContent).toBe("Mon, Tue and Thu");
    expect(q("[data-testid=schedule-summary]").textContent).toContain("on Mon, Tue and Thu");
  });

  it("switches a rule with its switch and keeps the switch readable when it is off", () => {
    const onToggle = vi.fn();
    const rule = { ...ruleOf(NEW_WINDOW), enabled: false };
    act(() =>
      root.render(
        <ul>
          <RuleRow
            rule={rule}
            onToggle={onToggle}
            onEdit={vi.fn()}
            onDuplicate={vi.fn()}
            onDelete={vi.fn()}
          />
        </ul>,
      ),
    );
    expect(q("[data-testid=rule-name]").textContent).toBe("Overnight only");
    const sw = q("[data-testid=rule-switch]");
    expect(sw.getAttribute("aria-checked")).toBe("false");
    click(sw);
    expect(onToggle).toHaveBeenCalledWith(true);
    click(q("[data-testid=rule-menu-button]"));
    expect([...host.querySelectorAll("[role=menuitem]")].map((e) => e.textContent)).toEqual([
      "Edit",
      "Duplicate",
      "Delete",
    ]);
  });
});
