// Errors the window catches (render errors, uncaught errors, unhandled
// rejections) go to the app log through `log_frontend_error`, rate-limited so
// an error repeating in a render loop can't flood the log.

import { logFrontendError } from "./api";

/** At most this many reports per `REPORT_WINDOW_MS`; the rest are dropped. */
export const REPORT_LIMIT = 20;
export const REPORT_WINDOW_MS = 60_000;

/** One caught error, as the log records it. */
export interface FrontendError {
  message: string;
  stack: string | null;
  componentStack: string | null;
  /** Where it was caught: `render`, `error` or `unhandledrejection`. */
  source: string;
}

/**
 * A reporter that passes at most `REPORT_LIMIT` errors per window to `send`
 * and returns whether this one went. A failed send is ignored: reporting must
 * never raise another error.
 */
export function createReporter(
  send: (error: FrontendError) => Promise<unknown>,
  now: () => number = Date.now,
): (error: FrontendError) => boolean {
  const sent: number[] = [];
  return (error) => {
    const t = now();
    while (sent.length > 0 && t - sent[0] >= REPORT_WINDOW_MS) sent.shift();
    if (sent.length >= REPORT_LIMIT) return false;
    sent.push(t);
    void Promise.resolve()
      .then(() => send(error))
      .catch(() => {});
    return true;
  };
}

/** The app's reporter, sending to the backend log. */
export const reportError = createReporter((e) =>
  logFrontendError(e.message, e.stack, e.componentStack, e.source),
);

/** A thrown value's message (`TypeError: …` for an Error) and its stack, if any. */
export function describeThrown(value: unknown): { message: string; stack: string | null } {
  if (value instanceof Error) {
    const message = value.message ? `${value.name}: ${value.message}` : value.name;
    return { message, stack: value.stack ?? null };
  }
  if (typeof value === "string") return { message: value, stack: null };
  try {
    return { message: JSON.stringify(value) ?? String(value), stack: null };
  } catch {
    return { message: String(value), stack: null };
  }
}

/**
 * Reports `error` events (uncaught errors) and `unhandledrejection`s on
 * `target`; returns a function that stops.
 */
export function reportUncaught(
  report: (error: FrontendError) => boolean = reportError,
  target: Window = window,
): () => void {
  const onError = (e: ErrorEvent) => {
    const { message, stack } = describeThrown(e.error ?? e.message);
    report({ message, stack, componentStack: null, source: "error" });
  };
  const onRejection = (e: PromiseRejectionEvent) => {
    const { message, stack } = describeThrown(e.reason);
    report({ message, stack, componentStack: null, source: "unhandledrejection" });
  };
  target.addEventListener("error", onError);
  target.addEventListener("unhandledrejection", onRejection);
  return () => {
    target.removeEventListener("error", onError);
    target.removeEventListener("unhandledrejection", onRejection);
  };
}

/** A caught render error as Copy diagnostics adds it to the report. */
export function errorDetails(
  error: { message: string; stack: string | null },
  componentStack: string | null,
): string {
  const parts = [`Window error: ${error.message}`];
  // A V8 stack starts with the message again; keep only the frames.
  const frames = error.stack?.startsWith(error.message)
    ? error.stack.slice(error.message.length).replace(/^\n/, "")
    : error.stack;
  if (frames?.trim()) parts.push(`Stack:\n${frames.trimEnd()}`);
  if (componentStack?.trim())
    parts.push(`Component stack:\n${componentStack.replace(/^\n+/, "").trimEnd()}`);
  return parts.join("\n");
}
