// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { snap } from "../../lib/testSnapshot";
import type { DownloadSnapshot } from "../../lib/types";
import type { LimitEditor } from "../../lib/useLimitEditor";
import { MissingBanner } from "../details/MissingBanner";
import { MissingFlag } from "./MissingFlag";
import type { RowHandlers } from "./rowHandlers";
import { RowActions } from "./RowActions";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const editor: LimitEditor = {
  draft: null,
  globalLimit: null,
  open: vi.fn(),
  change: vi.fn(),
  commit: vi.fn(),
  cancel: vi.fn(),
  buttonRef: { current: null },
};

const missing = (overrides: Partial<DownloadSnapshot> = {}) =>
  snap({
    status: "completed",
    file_missing: true,
    filename: "gone.iso",
    final_path: "C:\\Downloads\\gone.iso",
    ...overrides,
  });

describe("missing file UI", () => {
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

  it("flags a row with an icon and the words, not colour alone", () => {
    act(() => root.render(<MissingFlag />));
    expect(host.textContent).toBe("File missing");
    expect(host.querySelector("svg")).not.toBeNull();
  });

  it("swaps a missing row's Open and Show in folder for Locate…, which calls its handler", () => {
    const handlers = { locate: vi.fn() } as unknown as RowHandlers;
    const d = missing();
    act(() =>
      root.render(<RowActions d={d} lingering={false} editor={editor} handlers={handlers} />),
    );
    const names = [...host.querySelectorAll("button")].map((b) => b.getAttribute("aria-label"));
    expect(names).toEqual(["Locate…", "Archive"]);
    act(() => host.querySelector<HTMLButtonElement>('button[aria-label="Locate…"]')!.click());
    expect(handlers.locate).toHaveBeenCalledWith(d);
  });

  it("shows the old path in the banner, with Locate… and Remove from list", () => {
    const onLocate = vi.fn();
    const onRemove = vi.fn();
    const d = missing();
    act(() => root.render(<MissingBanner d={d} onLocate={onLocate} onRemove={onRemove} />));
    expect(host.querySelector('[data-testid="missing-path"]')?.textContent).toBe(
      "C:\\Downloads\\gone.iso",
    );
    const buttons = [...host.querySelectorAll("button")];
    expect(buttons.map((b) => b.textContent)).toEqual(["Locate…", "Remove from list"]);
    // One primary per view: Remove from list is the quiet text action.
    expect(buttons[0].classList.contains("btn-primary")).toBe(true);
    expect(buttons[1].classList.contains("btn")).toBe(false);
    act(() => buttons[0].click());
    expect(onLocate).toHaveBeenCalledWith(d);
    expect(onRemove).not.toHaveBeenCalled();
    act(() => buttons[1].click());
    expect(onRemove).toHaveBeenCalledWith(d);
  });
});
