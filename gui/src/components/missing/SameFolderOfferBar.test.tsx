// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SameFolderOfferBar } from "./SameFolderOfferBar";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("SameFolderOfferBar", () => {
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

  it("shows nothing when there is no offer", () => {
    act(() =>
      root.render(<SameFolderOfferBar offer={null} onAccept={vi.fn()} onDismiss={vi.fn()} />),
    );
    expect(host.innerHTML).toBe("");
  });

  it("names the count and both folders, and relinks only on a click", () => {
    const onAccept = vi.fn();
    const onDismiss = vi.fn();
    const offer = { count: 3, old_folder: "/old", new_folder: "/new" };
    act(() =>
      root.render(<SameFolderOfferBar offer={offer} onAccept={onAccept} onDismiss={onDismiss} />),
    );
    const text = host.textContent ?? "";
    expect(text).toContain("3 other missing downloads");
    expect(text).toContain("/old");
    expect(text).toContain("/new");
    expect(onAccept).not.toHaveBeenCalled();
    const [accept, dismiss] = [...host.querySelectorAll("button")];
    expect(accept.textContent).toBe("Relink all 3");
    act(() => accept.click());
    expect(onAccept).toHaveBeenCalledOnce();
    act(() => dismiss.click());
    expect(onDismiss).toHaveBeenCalledOnce();
  });
});
