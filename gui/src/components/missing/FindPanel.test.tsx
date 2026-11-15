// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ActiveSearch } from "../../lib/find";
import { snap } from "../../lib/testSnapshot";
import type { FindResult, FoundFile } from "../../lib/types";
import { FindPanel, type FindControls } from "./FindPanel";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const d = snap({ id: 1, status: "completed", file_missing: true, final_path: "/old/a.iso" });
const found = (path: string, reason: FoundFile["reason"] = "SameNameAndSize"): FoundFile => ({
  path,
  size: 2048,
  modified: 1_700_000_000,
  reason,
});
const result = (matches: FoundFile[], truncated = false): FindResult => ({
  id: 1,
  matches,
  searched_folders: ["/old", "/other", "/dl", "/archive"],
  truncated,
});
const searching: ActiveSearch = {
  search: 1,
  ids: [1],
  many: false,
  foldersDone: 2,
  foldersTotal: 4,
  current: "/dl",
};

describe("FindPanel", () => {
  let host: HTMLDivElement;
  let root: Root;
  const actions = {
    onFind: vi.fn(),
    onCancel: vi.fn(),
    onRelink: vi.fn(),
    onSkip: vi.fn(),
    onLocate: vi.fn(),
    onForget: vi.fn(),
  };
  const show = (c: Partial<FindControls>) =>
    act(() =>
      root.render(
        <FindPanel
          d={d}
          searching={null}
          result={undefined}
          locateError={null}
          {...actions}
          {...c}
        />,
      ),
    );
  const buttons = () => [...host.querySelectorAll("button")];
  const button = (label: string) => buttons().find((b) => b.textContent?.includes(label))!;

  beforeEach(() => {
    Object.values(actions).forEach((f) => f.mockReset());
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });
  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("shows nothing without a search or a result", () => {
    show({});
    expect(host.innerHTML).toBe("");
  });

  it("shows progress, the current folder and Cancel while searching", () => {
    show({ searching });
    expect(host.textContent).toContain(`Searching for ${d.filename}`);
    expect(host.textContent).toContain("2 of 4 folders");
    expect(host.textContent).toContain("/dl");
    expect(host.querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow")).toBe("2");
    act(() => button("Cancel").click());
    expect(actions.onCancel).toHaveBeenCalledOnce();
  });

  it("offers candidates to pick from, with Relink and Skip", () => {
    const [a, b] = [found("/new/a.iso"), found("/desk/a (1).iso", "RenamedUniqueSize")];
    show({ result: result([a, b]) });
    expect(host.textContent).toContain("2 possible matches found");
    expect(host.textContent).toContain("Same name and size");
    expect(host.textContent).toContain("Possible match: renamed");
    const radios = [...host.querySelectorAll('[role="radio"]')];
    expect(radios.map((r) => r.getAttribute("aria-checked"))).toEqual(["true", "false"]);
    act(() => (radios[1] as HTMLElement).click());
    act(() => button("Relink").click());
    // Nothing is relinked until Relink, and then it is the picked candidate.
    expect(actions.onRelink).toHaveBeenCalledExactlyOnceWith(d, b);
    act(() => button("Skip").click());
    expect(actions.onSkip).toHaveBeenCalledWith(d);
  });

  it("shows a single match with Relink and Locate…", () => {
    const only = found("/new/a.iso");
    show({ result: result([only]) });
    expect(host.textContent).toContain("Match found");
    expect(host.querySelector('[role="radio"]')).toBeNull();
    expect(buttons().map((b) => b.textContent)).toEqual(["Relink", "Locate…"]);
    act(() => button("Relink").click());
    expect(actions.onRelink).toHaveBeenCalledWith(d, only);
    act(() => button("Locate…").click());
    expect(actions.onLocate).toHaveBeenCalledWith(d);
  });

  it("says no match, lists the folders searched, and offers Locate…, Search again and Remove", () => {
    show({ result: result([]) });
    expect(host.textContent).toContain("No match found");
    expect(host.textContent).toContain("4 folders searched");
    expect(host.querySelectorAll("li")).toHaveLength(4);
    expect(host.querySelector("li svg")).toBeNull();
    expect(host.textContent).not.toContain("Not every folder");
    act(() => button("Search again").click());
    expect(actions.onFind).toHaveBeenCalledWith(d);
    act(() => button("Locate…").click());
    expect(actions.onLocate).toHaveBeenCalledWith(d);
    act(() => button("Remove from list").click());
    expect(actions.onForget).toHaveBeenCalledWith(d);
  });

  it("notes quietly when a search was cut short, and shows a refused Locate… beside it", () => {
    show({ result: result([], true), locateError: "That file is a different size." });
    expect(host.textContent).toContain("Not every folder was fully searched");
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      "That file is a different size.",
    );
  });
});
