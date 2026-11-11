// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { snap } from "../../lib/testSnapshot";
import type { DownloadSnapshot } from "../../lib/types";
import type { LimitEditor } from "../../lib/useLimitEditor";
import type { RowHandlers } from "./rowHandlers";
import { RowActions } from "./RowActions";
import { SpeedCell } from "./SpeedCell";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const KIB = 1024;

const editor = (overrides: Partial<LimitEditor> = {}): LimitEditor => ({
  draft: null,
  globalLimit: null,
  open: vi.fn(),
  change: vi.fn(),
  commit: vi.fn(),
  cancel: vi.fn(),
  buttonRef: { current: null },
  ...overrides,
});

describe("SpeedCell", () => {
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

  const render = (d: DownloadSnapshot, e = editor()) =>
    act(() => root.render(<SpeedCell d={d} editor={e} />));
  const button = () => host.querySelector<HTMLButtonElement>('[data-testid="row-limit"]');
  // Both limit forms are in the DOM (CSS picks one), so read the full one alone.
  const fullText = () => {
    const copy = host.cloneNode(true) as HTMLElement;
    copy.querySelectorAll(".lim-short").forEach((s) => s.remove());
    return copy.textContent;
  };

  it("shows just the speed for a row without a limit", () => {
    render(snap({ status: "downloading", bytes_per_sec: 11.4 * KIB * KIB }));
    expect(button()).toBeNull();
    expect(host.textContent).toBe("11.4 MB/s");
  });

  it("reads a limited row's speed against its limit, with no gauge", () => {
    const e = editor();
    render(snap({ status: "downloading", bytes_per_sec: 49.8 * KIB, limit: 50 * KIB }), e);
    expect(fullText()).toBe("49.8 / 50 KB/s");
    expect(host.querySelector(".lim-short")?.textContent).toBe("50K");
    expect(host.querySelector("svg")).toBeNull();
    expect(button()?.getAttribute("aria-label")).toBe(
      "49.8 KB/s, limited to 50 KB/s. Change speed limit",
    );
    act(() => button()!.click());
    expect(e.open).toHaveBeenCalledOnce();
  });

  it("shows a dash before the first reading and the cap while paused or waiting", () => {
    render(snap({ status: "downloading", bytes_per_sec: 0, limit: 50 * KIB }));
    expect(fullText()).toBe("– / 50 KB/s");
    for (const status of ["paused", "queued"] as const) {
      render(snap({ status, limit: 50 * KIB }));
      expect(fullText()).toBe("≤ 50 KB/s");
      expect(button()?.getAttribute("title")).toBe("Limited to 50 KB/s. Change speed limit");
    }
  });

  it("drops the limit while the editor is open, and on finished rows", () => {
    const d = snap({ status: "downloading", bytes_per_sec: 49.8 * KIB, limit: 50 * KIB });
    render(d, editor({ draft: { amount: "50", unit: "K" } }));
    expect(button()).toBeNull();
    expect(host.textContent).toBe("49.8 KB/s");
    render(snap({ status: "completed", limit: 50 * KIB }));
    expect(host.textContent).toBe("");
  });
});

describe("the row's speed-limit action", () => {
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

  const gauge = (d: DownloadSnapshot) => {
    act(() =>
      root.render(
        <RowActions d={d} lingering={false} editor={editor()} handlers={{} as RowHandlers} />,
      ),
    );
    return host.querySelector<HTMLButtonElement>('button[aria-label="Speed limit"]')!;
  };

  it("is tinted and names the limit while one is set", () => {
    const set = gauge(snap({ status: "downloading", limit: 50 * KIB }));
    expect(set.classList.contains("is-set")).toBe(true);
    expect(set.title).toBe("Speed limit: 50 KB/s");

    const unset = gauge(snap({ status: "downloading" }));
    expect(unset.classList.contains("is-set")).toBe(false);
    expect(unset.title).toBe("Speed limit");
  });
});
