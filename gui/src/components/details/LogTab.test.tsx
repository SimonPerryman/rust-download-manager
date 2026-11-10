// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LogLine } from "../../lib/types";

const api = vi.hoisted(() => ({ downloadLog: vi.fn(), saveTextFile: vi.fn() }));
vi.mock("../../lib/api", () => api);

const { LogTab } = await import("./LogTab");
const { copyText } = await import("../../lib/log");
const { LOG_DETAILED_KEY } = await import("../../lib/prefs");
const { COPIED_MS } = await import("../../lib/useCopied");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const LINES: LogLine[] = [
  {
    at: Date.UTC(2026, 10, 10, 9, 0, 0),
    level: "debug",
    message: "Requested the file",
    fields: "",
  },
  { at: Date.UTC(2026, 10, 10, 9, 0, 1), level: "info", message: "Started", fields: "" },
  { at: Date.UTC(2026, 10, 10, 9, 0, 2), level: "debug", message: "Lane 2 hand-off", fields: "" },
  { at: Date.UTC(2026, 10, 10, 9, 0, 3), level: "info", message: "Completed", fields: "" },
];

describe("LogTab", () => {
  let host: HTMLDivElement;
  let root: Root;
  let writeText: ReturnType<typeof vi.fn>;

  const button = (testid: string) =>
    host.querySelector<HTMLButtonElement>(`[data-testid="${testid}"]`)!;

  beforeEach(async () => {
    localStorage.removeItem(LOG_DETAILED_KEY);
    api.downloadLog.mockReset().mockResolvedValue(LINES);
    api.saveTextFile.mockReset().mockResolvedValue("/tmp/ubuntu.iso.log");
    writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    await act(async () => root.render(<LogTab id={1} name="ubuntu.iso" />));
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("shows the simple lines by default, with the switch saying so", () => {
    expect(host.querySelectorAll("li")).toHaveLength(2);
    expect(button("log-simple").getAttribute("aria-pressed")).toBe("true");
    expect(button("log-detailed").getAttribute("aria-pressed")).toBe("false");
  });

  it("switches to the detailed lines and remembers it", async () => {
    await act(async () => button("log-detailed").click());
    expect(host.querySelectorAll("li")).toHaveLength(4);
    expect(button("log-detailed").getAttribute("aria-pressed")).toBe("true");
    expect(localStorage.getItem(LOG_DETAILED_KEY)).toBe("true");
    await act(async () => button("log-simple").click());
    expect(host.querySelectorAll("li")).toHaveLength(2);
    expect(localStorage.getItem(LOG_DETAILED_KEY)).toBe("false");
  });

  it("copies every line, the detail too, with Simple shown", async () => {
    await act(async () => button("log-copy").click());
    expect(writeText).toHaveBeenCalledWith(copyText(LINES));
    expect(writeText.mock.calls[0][0]).toContain("Lane 2 hand-off");
    expect(button("log-copy").textContent).toBe("Copied");
    expect(host.querySelector('[role="status"]')?.textContent).toBe("Copied 4 lines");
  });

  it("saves the same text as <name>.log", async () => {
    expect(button("log-save").textContent).toBe("Save…");
    expect(button("log-save").title).toBe("Save the log as a file");
    await act(async () => button("log-save").click());
    expect(api.saveTextFile).toHaveBeenCalledWith("ubuntu.iso.log", copyText(LINES));
    expect(button("log-save").textContent).toBe("Saved");
    expect(host.querySelector('[role="status"]')?.textContent).toBe("Saved to /tmp/ubuntu.iso.log");
  });

  it("says each confirmation once, and only until it lapses", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    try {
      const status = () => host.querySelector('[role="status"]')?.textContent;
      await act(async () => button("log-save").click());
      await act(async () => button("log-copy").click());
      expect(status()).toBe("Copied 4 lines");
      await act(async () => vi.advanceTimersByTime(COPIED_MS));
      // The earlier save isn't said again once Copy's confirmation goes.
      expect(status()).toBe("");
      expect(button("log-copy").textContent).toBe("Copy");
      expect(button("log-save").textContent).toBe("Save…");
    } finally {
      vi.useRealTimers();
    }
  });

  it("offers Copy and Save while only detailed lines exist", async () => {
    const detail = LINES.filter((l) => l.level === "debug");
    api.downloadLog.mockResolvedValue(detail);
    await act(async () => root.render(<LogTab key={2} id={2} name="early.iso" />));
    expect(host.querySelector('[data-testid="log-only-detail"]')?.textContent).toBe(
      "Only detailed lines so far. Choose Detailed to see them.",
    );
    expect(button("log-simple").getAttribute("aria-pressed")).toBe("true");
    await act(async () => button("log-copy").click());
    expect(writeText).toHaveBeenCalledWith(copyText(detail));
    await act(async () => button("log-save").click());
    expect(api.saveTextFile).toHaveBeenCalledWith("early.iso.log", copyText(detail));
  });

  it("restarts Copied when Copy is clicked again", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    try {
      await act(async () => button("log-copy").click());
      await act(async () => vi.advanceTimersByTime(COPIED_MS - 500));
      await act(async () => button("log-copy").click());
      await act(async () => vi.advanceTimersByTime(1000));
      expect(button("log-copy").textContent).toBe("Copied");
      await act(async () => vi.advanceTimersByTime(500));
      expect(button("log-copy").textContent).toBe("Copy");
    } finally {
      vi.useRealTimers();
    }
  });

  it("says why a save failed", async () => {
    api.saveTextFile.mockRejectedValue("disk full");
    await act(async () => button("log-save").click());
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      "Couldn't save the log: disk full",
    );
  });
});
