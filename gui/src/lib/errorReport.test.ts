// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const logFrontendError = vi.fn();
vi.mock("./api", () => ({ logFrontendError: (...args: unknown[]) => logFrontendError(...args) }));

const {
  REPORT_LIMIT,
  REPORT_WINDOW_MS,
  createReporter,
  describeThrown,
  errorDetails,
  reportUncaught,
} = await import("./errorReport");

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

/** A PromiseRejectionEvent, built by hand where the environment lacks one. */
function rejection(reason: unknown): Event {
  if (typeof PromiseRejectionEvent === "function") {
    const promise = Promise.reject(reason);
    promise.catch(() => {});
    return new PromiseRejectionEvent("unhandledrejection", { promise, reason });
  }
  return Object.assign(new Event("unhandledrejection"), { reason });
}

describe("reportUncaught", () => {
  let stop: () => void;

  beforeEach(() => {
    logFrontendError.mockReset().mockResolvedValue(undefined);
    // A fresh limit per test: the module's own reporter is shared.
    const report = createReporter((e) =>
      logFrontendError(e.message, e.stack, e.componentStack, e.source),
    );
    stop = reportUncaught(report);
  });

  afterEach(() => stop());

  it("logs an uncaught error and an unhandled rejection", async () => {
    window.dispatchEvent(
      new ErrorEvent("error", { error: new RangeError("too far"), message: "too far" }),
    );
    window.dispatchEvent(rejection(new Error("no reply")));
    await flush();

    expect(logFrontendError).toHaveBeenCalledTimes(2);
    const [error, rejected] = logFrontendError.mock.calls;
    expect(error[0]).toBe("RangeError: too far");
    expect(error[1]).toContain("too far");
    expect(error[2]).toBeNull();
    expect(error[3]).toBe("error");
    expect(rejected[0]).toBe("Error: no reply");
    expect(rejected[3]).toBe("unhandledrejection");
  });

  it("logs at most the limit from a burst of 50", async () => {
    for (let i = 0; i < 50; i++) {
      window.dispatchEvent(new ErrorEvent("error", { error: new Error(`loop ${i}`) }));
    }
    await flush();
    expect(logFrontendError).toHaveBeenCalledTimes(REPORT_LIMIT);
    expect(REPORT_LIMIT).toBeLessThanOrEqual(20);
  });

  it("stops listening once stopped", async () => {
    stop();
    // Handled here, so the test environment doesn't take it as uncaught.
    const handled = (e: Event) => e.preventDefault();
    window.addEventListener("error", handled);
    try {
      window.dispatchEvent(
        new ErrorEvent("error", { error: new Error("after"), cancelable: true }),
      );
      await flush();
    } finally {
      window.removeEventListener("error", handled);
    }
    expect(logFrontendError).not.toHaveBeenCalled();
  });
});

describe("createReporter", () => {
  it("lets reports through again once the window has passed", () => {
    let now = 0;
    const send = vi.fn().mockResolvedValue(undefined);
    const report = createReporter(send, () => now);
    const error = { message: "m", stack: null, componentStack: null, source: "error" };
    const went = Array.from({ length: REPORT_LIMIT + 5 }, () => report(error));
    expect(went.filter(Boolean)).toHaveLength(REPORT_LIMIT);
    now = REPORT_WINDOW_MS - 1;
    expect(report(error)).toBe(false);
    now = REPORT_WINDOW_MS;
    expect(report(error)).toBe(true);
  });

  it("swallows a failed send", async () => {
    const send = vi.fn().mockRejectedValue(new Error("backend gone"));
    const report = createReporter(send);
    expect(report({ message: "m", stack: null, componentStack: null, source: "error" })).toBe(true);
    await flush();
    expect(send).toHaveBeenCalledTimes(1);
  });
});

describe("describeThrown", () => {
  it("names an Error by its type and message", () => {
    const e = new TypeError("bad");
    expect(describeThrown(e)).toEqual({ message: "TypeError: bad", stack: e.stack });
  });

  it("takes other values as text", () => {
    expect(describeThrown("plain")).toEqual({ message: "plain", stack: null });
    expect(describeThrown({ code: 5 })).toEqual({ message: '{"code":5}', stack: null });
    expect(describeThrown(undefined)).toEqual({ message: "undefined", stack: null });
  });
});

describe("errorDetails", () => {
  it("lists the error, its frames and its component stack", () => {
    const text = errorDetails(
      { message: "TypeError: bad", stack: "TypeError: bad\n    at f (a.js:1)" },
      "\n    at Pane\n    at App",
    );
    expect(text).toBe(
      "Window error: TypeError: bad\nStack:\n    at f (a.js:1)\nComponent stack:\n    at Pane\n    at App",
    );
  });

  it("leaves out what it doesn't have", () => {
    expect(errorDetails({ message: "boom", stack: null }, null)).toBe("Window error: boom");
  });
});
