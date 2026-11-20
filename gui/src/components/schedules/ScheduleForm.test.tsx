// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { draftOf, NEW_WINDOW, ruleOf, speedOf, NEW_SPEED } from "../../lib/schedule";
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

  const typeInto = (el: HTMLInputElement, text: string) =>
    act(() => {
      const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
      set.call(el, text);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    });
  const key = (el: Element, k: string) =>
    act(() => {
      el.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true }));
    });
  const withChoices = (initial = NEW_WINDOW) => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    act(() =>
      root.render(
        <ScheduleForm
          initial={initial}
          editing={false}
          choices={{
            tags: ["course-2", "course-3", "work"],
            hosts: ["cdn.example.com"],
            batches: [{ id: 3, label: "Course" }],
          }}
          onSubmit={onSubmit}
          onClose={vi.fn()}
        />,
      ),
    );
    return onSubmit;
  };

  it("collapses empty scope rows to + Add and opens one on click", () => {
    form();
    expect(host.querySelector("input[aria-label=Tags]")).toBeNull();
    expect(host.querySelectorAll("[data-testid^=scope-add-]").length).toBe(3);
    click(q("[data-testid=scope-add-tag]"));
    expect(host.querySelector("input[aria-label=Tags]")).not.toBeNull();
    expect(host.querySelector("[data-testid=scope-add-tag]")).toBeNull();
  });

  it("suggests existing tags as you type and accepts a new one, noting it never blocks saving", () => {
    const onSubmit = withChoices();
    click(q("[data-testid=scope-add-tag]"));
    const input = q<HTMLInputElement>("input[aria-label=Tags]");
    typeInto(input, "course");
    expect(
      [...host.querySelectorAll("[data-testid=scope-option]")].map((o) => o.textContent),
    ).toEqual(["course-2", "course-3", 'Add "course"']);
    click(host.querySelectorAll("[data-testid=scope-option]")[0]);
    typeInto(q<HTMLInputElement>("input[aria-label=Tags]"), "course-4");
    key(q("input[aria-label=Tags]"), "Enter");
    expect(
      [...host.querySelectorAll("[data-testid=scope-chip]")].map((c) =>
        c.getAttribute("data-name"),
      ),
    ).toEqual(["course-2", "course-4"]);
    expect(q("[data-testid=scope-warning]").textContent).toBe("No downloads have the tag course-4");
    expect(q("[data-testid=schedule-summary]").textContent).toBe(
      "Downloads tagged course-2 or course-4 run from 22:00 to 08:00 every day, and pause at 08:00.",
    );
    click(q("[data-testid=schedule-submit]"));
    expect(onSubmit.mock.calls[0][0]).toMatchObject({ scopeTags: ["course-2", "course-4"] });
  });

  it("removes a chip, and takes several hosts and a batch from its list only", () => {
    const onSubmit = withChoices({ ...NEW_WINDOW, scopeTags: ["work"] });
    click(q("[aria-label='Remove work']"));
    expect(host.querySelector("[data-testid=scope-chip]")).toBeNull();
    click(q("[data-testid=scope-add-host]"));
    const hostBox = q<HTMLInputElement>("input[aria-label=Hosts]");
    typeInto(hostBox, "Other.org");
    key(hostBox, "Enter");
    typeInto(q<HTMLInputElement>("input[aria-label=Hosts]"), "cdn");
    click(q("[data-testid=scope-option]"));
    click(q("[data-testid=scope-add-batch]"));
    const batchBox = q<HTMLInputElement>("input[aria-label=Batches]");
    typeInto(batchBox, "nope");
    key(batchBox, "Enter");
    expect(host.querySelector("[data-testid=scope-batch] [data-testid=scope-chip]")).toBeNull();
    typeInto(q<HTMLInputElement>("input[aria-label=Batches]"), "cou");
    click(q("[data-testid=scope-option]"));
    click(q("[data-testid=schedule-submit]"));
    expect(onSubmit.mock.calls[0][0]).toMatchObject({
      scopeTags: [],
      scopeHosts: ["other.org", "cdn.example.com"],
      scopeBatches: [{ id: 3, label: "Course" }],
    });
  });

  it("changes the sentence when running downloads are left to finish", () => {
    form();
    click(q("[data-testid=finish-running]"));
    expect(q("[data-testid=schedule-summary]").textContent).toBe(
      "New downloads start from 22:00 to 08:00 every day. Running ones finish.",
    );
  });

  it("switches to a Speed limit: its fields, its sentence, and the entry it saves", () => {
    const onSubmit = form();
    pick(q("select[aria-label=Type]"), "speed");
    expect(host.querySelector("[data-testid=finish-running]")).toBeNull();
    expect(q<HTMLInputElement>("input[aria-label=Limit]").value).toBe("5");
    expect(q<HTMLInputElement>("input[aria-label=Name]").placeholder).toBe("5 MB/s limit");
    expect(q<HTMLInputElement>("input[aria-label='Start time']").value).toBe("08:00");
    expect(q("[data-testid=schedule-summary]").textContent).toBe(
      "Downloads are limited to 5 MB/s from 08:00 to 18:00 on weekdays.",
    );
    const limit = q<HTMLInputElement>("input[aria-label=Limit]");
    act(() => {
      const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
      set.call(limit, "0");
      limit.dispatchEvent(new Event("input", { bubbles: true }));
    });
    expect(q("[data-testid=schedule-summary]").textContent).toBe("Enter a speed above 0");
    expect(q<HTMLButtonElement>("[data-testid=schedule-submit]").disabled).toBe(true);
    act(() => {
      const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
      set.call(limit, "2.5");
      limit.dispatchEvent(new Event("input", { bubbles: true }));
    });
    click(q("[data-testid=schedule-submit]"));
    expect(onSubmit).toHaveBeenCalledWith(expect.objectContaining({ kind: "speed", limit: "2.5" }));
  });

  const type = (el: HTMLInputElement, text: string) =>
    act(() => {
      const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
      set.call(el, text);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    });

  it("offers KB/s and MB/s, and saves the limit in the unit picked", () => {
    const onSubmit = form();
    pick(q("select[aria-label=Type]"), "speed");
    const unit = q<HTMLSelectElement>("select[aria-label=Unit]");
    expect([...unit.options].map((o) => o.textContent)).toEqual(["KB/s", "MB/s"]);
    expect(unit.value).toBe("M");
    type(q("input[aria-label=Limit]"), "200");
    pick(unit, "K");
    expect(q("[data-testid=schedule-summary]").textContent).toBe(
      "Downloads are limited to 200 KB/s from 08:00 to 18:00 on weekdays.",
    );
    expect(q<HTMLInputElement>("input[aria-label=Name]").placeholder).toBe("200 KB/s limit");
    click(q("[data-testid=schedule-submit]"));
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({ kind: "speed", limit: "200", limitUnit: "K" }),
    );
  });

  it("shows an edited limit in its best unit", () => {
    act(() =>
      root.render(
        <ScheduleForm
          initial={draftOf(speedOf({ ...NEW_SPEED, limit: "200", limitUnit: "K" }))}
          editing
          onSubmit={vi.fn()}
          onClose={vi.fn()}
        />,
      ),
    );
    expect(q<HTMLInputElement>("input[aria-label=Limit]").value).toBe("200");
    expect(q<HTMLSelectElement>("select[aria-label=Unit]").value).toBe("K");
  });

  it("takes times typed loosely, tidies them on blur and saves them as HH:MM", () => {
    const onSubmit = form();
    const start = q<HTMLInputElement>("input[aria-label='Start time']");
    type(start, "930pm");
    expect(q("[data-testid=schedule-summary]").textContent).toBe(
      "Downloads run from 21:30 to 08:00 every day, and pause at 08:00.",
    );
    act(() => start.dispatchEvent(new FocusEvent("focusout", { bubbles: true })));
    expect(start.value).toBe("21:30");
    type(q("input[aria-label='End time']"), "7");
    click(q("[data-testid=schedule-submit]"));
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({ start: "21:30", end: "07:00" }),
    );
    type(start, "later");
    expect(q("[data-testid=schedule-summary]").textContent).toBe("Times look like 22:00");
  });

  it("keeps an edited rule's type fixed", () => {
    act(() =>
      root.render(
        <ScheduleForm
          initial={{ ...NEW_SPEED, limit: "3" }}
          editing={true}
          onSubmit={vi.fn()}
          onClose={vi.fn()}
        />,
      ),
    );
    expect(q<HTMLSelectElement>("select[aria-label=Type]").disabled).toBe(true);
    expect(q("[data-testid=schedule-submit]").textContent).toBe("Save");
  });

  it("shows a speed rule as its row", () => {
    act(() =>
      root.render(
        <ul>
          <RuleRow
            rule={speedOf({ ...NEW_SPEED, name: "Daytime cap" })}
            onToggle={vi.fn()}
            onEdit={vi.fn()}
            onDuplicate={vi.fn()}
            onDelete={vi.fn()}
          />
        </ul>,
      ),
    );
    expect(q("[data-testid=rule-name]").textContent).toBe("Daytime cap");
    expect(q("[data-testid=rule-summary]").textContent).toBe("5 MB/s, 08:00\u201318:00, weekdays");
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

  it("shows a rule's names as chips, +N more on hover, and warns when nothing matches", () => {
    const rule = ruleOf({ ...NEW_WINDOW, scopeTags: ["a", "b", "c"], scopeHosts: ["gone.org"] });
    act(() =>
      root.render(
        <ul>
          <RuleRow
            rule={rule}
            choices={{ tags: ["a", "b", "c"], hosts: [], batches: [] }}
            onToggle={vi.fn()}
            onEdit={vi.fn()}
            onDuplicate={vi.fn()}
            onDelete={vi.fn()}
          />
        </ul>,
      ),
    );
    expect(
      [...host.querySelectorAll("[data-testid=scope-name]")].map((n) => n.textContent),
    ).toEqual(["a", "b", "gone.org"]);
    expect(q("[data-testid=scope-more]").getAttribute("title")).toBe("a, b, c");
    expect(q("[data-testid=rule-warning]").textContent).toBe("No downloads are from gone.org");
    expect(host.querySelector("[data-testid=rule-warning-icon]")).not.toBeNull();
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
