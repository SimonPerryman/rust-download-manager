// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { batchGroups } from "../../lib/batches";
import { snap } from "../../lib/testSnapshot";
import type { DownloadSnapshot } from "../../lib/types";
import { BatchRow } from "./BatchRow";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const file = (id: number, over: Partial<DownloadSnapshot>) =>
  snap({ id, batch: 4, batch_label: "Course files", ...over });

describe("BatchRow", () => {
  let host: HTMLTableSectionElement;
  let root: Root;

  beforeEach(() => {
    const table = document.createElement("table");
    host = document.createElement("tbody");
    table.append(host);
    document.body.append(table);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.closest("table")!.remove();
  });

  const render = (ds: DownloadSnapshot[], collapsed = false) => {
    const onToggle = vi.fn();
    const onRetry = vi.fn();
    const onReveal = vi.fn();
    act(() =>
      root.render(
        <BatchRow
          group={batchGroups(ds)[0]}
          collapsed={collapsed}
          onToggle={onToggle}
          onRetry={onRetry}
          onReveal={onReveal}
        />,
      ),
    );
    return { onToggle, onRetry, onReveal };
  };
  const q = (id: string) => host.querySelector(`[data-testid="${id}"]`);

  it("says how far the batch is, with a bar", () => {
    render([
      file(1, { status: "completed", downloaded: 10, total: 10 }),
      file(2, { status: "queued" }),
    ]);
    expect(q("batch-name")!.textContent).toBe("Course files");
    expect(q("batch-rollup")!.textContent).toBe("1 of 2 done");
    expect(host.querySelector('[role="progressbar"]')!.getAttribute("aria-valuenow")).toBe("50");
  });

  it("offers Retry failed only while something failed, for that batch", () => {
    render([file(1, { status: "completed" }), file(2, { status: "queued" })]);
    expect(q("batch-retry")).toBeNull();
    expect(q("batch-failed")).toBeNull();

    const { onRetry } = render([file(1, { status: "failed" }), file(2, { status: "failed" })]);
    expect(q("batch-failed")!.textContent).toBe("2 failed");
    act(() => (q("batch-retry") as HTMLElement).click());
    expect(onRetry).toHaveBeenCalledWith(4);
  });

  it("puts the count right after the bar, and opens the batch's folder", () => {
    const { onReveal } = render([file(1, { status: "queued" })]);
    const cell = q("batch-rollup")!.closest("td")!;
    const bar = cell.querySelector('[role="progressbar"]')!;
    expect(bar.nextElementSibling).toBe(q("batch-rollup"));
    act(() => (q("batch-reveal") as HTMLElement).click());
    expect(onReveal).toHaveBeenCalledWith(4);
  });

  it("opens and closes with its chevron", () => {
    const { onToggle } = render([file(1, { status: "queued" })]);
    const chevron = host.querySelector("button")!;
    expect(chevron.getAttribute("aria-expanded")).toBe("true");
    act(() => chevron.click());
    expect(onToggle).toHaveBeenCalledWith(4);

    render([file(1, { status: "queued" })], true);
    expect(host.querySelector("button")!.getAttribute("aria-expanded")).toBe("false");
  });
});
