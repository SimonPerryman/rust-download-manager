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
      root.render(<SameFolderOfferBar offer={null} onSearch={vi.fn()} onDismiss={vi.fn()} />),
    );
    expect(host.innerHTML).toBe("");
  });

  it("asks whether to search the new folder, and searches only on a click", () => {
    const onSearch = vi.fn();
    const onDismiss = vi.fn();
    const offer = { count: 3, old_folder: "/old", new_folder: "/new" };
    act(() =>
      root.render(<SameFolderOfferBar offer={offer} onSearch={onSearch} onDismiss={onDismiss} />),
    );
    const text = host.textContent ?? "";
    expect(text).toBe("3 more missing files were in /old. Search /new for them?Search");
    expect(onSearch).not.toHaveBeenCalled();
    const [search, dismiss] = [...host.querySelectorAll("button")];
    expect(search.textContent).toBe("Search");
    expect(search.classList.contains("btn-primary")).toBe(true);
    act(() => search.click());
    expect(onSearch).toHaveBeenCalledOnce();
    act(() => dismiss.click());
    expect(onDismiss).toHaveBeenCalledOnce();
  });
});
