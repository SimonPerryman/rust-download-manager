// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { reviewRows } from "../../lib/find";
import { snap } from "../../lib/testSnapshot";
import type { FindResult, FoundFile } from "../../lib/types";
import { FindReviewPane } from "./FindReviewPane";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const gone = (id: number, name: string) =>
  snap({ id, filename: name, status: "completed", file_missing: true, final_path: `/old/${name}` });
const file = (path: string, reason: FoundFile["reason"] = "SameNameAndSize"): FoundFile => ({
  path,
  size: 10,
  modified: null,
  reason,
});
const result = (id: number, matches: FoundFile[], truncated = false): FindResult => ({
  id,
  matches,
  searched_folders: ["/new"],
  truncated,
});

describe("FindReviewPane", () => {
  let host: HTMLDivElement;
  let root: Root;
  const onRelink = vi.fn();
  const onLocate = vi.fn();
  const onClose = vi.fn();
  const downloads = [gone(1, "a.iso"), gone(2, "b.zip"), gone(3, "c.msi")];
  const results = {
    1: result(1, [file("/new/a.iso")]),
    2: result(2, [file("/new/b-copy.zip", "RenamedUniqueSize")]),
    3: result(3, []),
  };
  const show = (ids: number[] = [1, 2, 3], truncated = false) =>
    act(() =>
      root.render(
        <FindReviewPane
          rows={reviewRows(ids, downloads, {
            ...results,
            1: result(1, results[1].matches, truncated),
          })}
          height={300}
          onRelink={onRelink}
          onLocate={onLocate}
          onClose={onClose}
        />,
      ),
    );
  const checks = () => [...host.querySelectorAll<HTMLElement>('[role="checkbox"]')];
  const button = (label: string) =>
    [...host.querySelectorAll("button")].find((b) => b.textContent === label)!;

  beforeEach(() => {
    [onRelink, onLocate, onClose].forEach((f) => f.mockReset());
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });
  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("counts what was found and starts exact matches ticked, renamed ones not", () => {
    show();
    expect(host.querySelector('[data-testid="find-review-heading"]')?.textContent).toBe(
      "Found 2 of 3 files",
    );
    // The row without a match has no tick.
    expect(checks().map((c) => c.getAttribute("aria-checked"))).toEqual(["true", "false"]);
    expect(host.textContent).toContain("Not found");
  });

  it("relinks only the ticked rows, once Relink selected is clicked", () => {
    show();
    expect(onRelink).not.toHaveBeenCalled();
    act(() => button("Relink selected").click());
    expect(onRelink).toHaveBeenCalledExactlyOnceWith([{ id: 1, path: "/new/a.iso" }]);
    act(() => checks()[1].click());
    act(() => button("Relink selected").click());
    expect(onRelink).toHaveBeenLastCalledWith([
      { id: 1, path: "/new/a.iso" },
      { id: 2, path: "/new/b-copy.zip" },
    ]);
  });

  it("disables Relink selected when nothing is ticked", () => {
    show();
    act(() => checks()[0].click());
    expect(button("Relink selected").hasAttribute("disabled")).toBe(true);
  });

  it("gives every row a Locate… link", () => {
    show();
    const links = [...host.querySelectorAll("button")].filter((b) => b.textContent === "Locate…");
    expect(links).toHaveLength(3);
    act(() => links[2].click());
    expect(onLocate).toHaveBeenCalledWith(downloads[2]);
  });

  it("names the folder when every found file is in one", () => {
    show([1]);
    expect(host.querySelector('[data-testid="find-review-heading"]')?.textContent).toBe(
      "Found in /new",
    );
  });

  it("notes quietly when a folder wasn't fully searched, and closes", () => {
    show([1, 2, 3], true);
    expect(host.textContent).toContain("Not every folder was fully searched");
    act(() => button("Close").click());
    expect(onClose).toHaveBeenCalledOnce();
  });
});
