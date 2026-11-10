// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  logFrontendError: vi.fn(),
  logsFolder: vi.fn(),
  openLogsFolder: vi.fn(),
  openBugReport: vi.fn(),
  diagnostics: vi.fn(),
}));
vi.mock("../../lib/api", () => api);

const { ErrorBoundary } = await import("./ErrorBoundary");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Broken(): never {
  throw new TypeError("Cannot read properties of undefined (reading 'filename')");
}

describe("ErrorBoundary", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    for (const f of Object.values(api)) f.mockReset().mockResolvedValue(undefined);
    api.logsFolder.mockResolvedValue("/home/someone/.rdm");
    api.diagnostics.mockResolvedValue("rdm diagnostics");
    // React reports the caught error on the console too.
    vi.spyOn(console, "error").mockImplementation(() => {});
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.restoreAllMocks();
  });

  it("shows the fallback screen and logs the error once", async () => {
    await act(async () => {
      root.render(
        <ErrorBoundary>
          <Broken />
        </ErrorBoundary>,
      );
    });

    expect(host.querySelector("h1")?.textContent).toBe("Something went wrong");
    const buttons = [...host.querySelectorAll("button")].map((b) => b.textContent);
    expect(buttons).toEqual(
      expect.arrayContaining(["Reload", "Open logs folder", "Copy diagnostics"]),
    );
    // The technical text waits behind Show details.
    const details = host.querySelector("details")!;
    expect(details.open).toBe(false);
    expect(details.querySelector("summary")?.textContent).toBe("Show details");
    // It takes focus, so keyboard users land on it.
    expect(document.activeElement).toBe(host.querySelector("h1"));
    await act(async () => details.querySelector("summary")!.click());
    expect(details.open).toBe(true);
    expect(details.querySelector("pre")?.textContent).toBe(
      "TypeError: Cannot read properties of undefined (reading 'filename')",
    );
    expect(host.querySelector('[data-testid="open-logs-folder"]')?.getAttribute("title")).toBe(
      "/home/someone/.rdm",
    );

    expect(api.logFrontendError).toHaveBeenCalledTimes(1);
    const [message, stack, componentStack, source] = api.logFrontendError.mock.calls[0];
    expect(message).toBe("TypeError: Cannot read properties of undefined (reading 'filename')");
    expect(stack).toContain("Broken");
    expect(componentStack).toContain("Broken");
    expect(source).toBe("render");
  });

  it("copies diagnostics with the error and its component stack", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    await act(async () => {
      root.render(
        <ErrorBoundary>
          <Broken />
        </ErrorBoundary>,
      );
    });
    await act(async () => {
      host.querySelector<HTMLButtonElement>('[data-testid="copy-diagnostics"]')!.click();
    });

    expect(api.diagnostics).toHaveBeenCalledTimes(1);
    const extra: string = api.diagnostics.mock.calls[0][0];
    expect(extra).toMatch(/^Window error: TypeError: Cannot read properties/);
    expect(extra).toContain("Component stack:");
    expect(extra).toContain("Broken");
    expect(writeText).toHaveBeenCalledWith("rdm diagnostics");
    expect(host.querySelector('[data-testid="copy-diagnostics"]')?.textContent).toBe("Copied");
  });

  it("opens the bug report form, not a URL of the page's choosing", async () => {
    await act(async () => {
      root.render(
        <ErrorBoundary>
          <Broken />
        </ErrorBoundary>,
      );
    });
    const link = [...host.querySelectorAll("button")].find(
      (b) => b.textContent === "new issue on GitHub",
    )!;
    await act(async () => link.click());
    expect(api.openBugReport).toHaveBeenCalledWith();
  });

  it("renders its children when nothing throws", async () => {
    await act(async () => {
      root.render(
        <ErrorBoundary>
          <p>fine</p>
        </ErrorBoundary>,
      );
    });
    expect(host.textContent).toBe("fine");
    expect(api.logFrontendError).not.toHaveBeenCalled();
  });
});
