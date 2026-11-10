import { useEffect, useState } from "react";

/** Dispatched on `window` (a `CustomEvent` whose `detail` is the message). */
export const RENDER_ERROR_EVENT = "rdm-test-render-error";

/**
 * A test hook: on `RENDER_ERROR_EVENT` its next render throws, so the e2e
 * suite can bring up the fallback screen in a real window. Harmless outside
 * tests: all it can do is show that screen.
 */
export function RenderErrorProbe() {
  const [message, setMessage] = useState<string | null>(null);
  useEffect(() => {
    const onEvent = (e: Event) =>
      setMessage(String((e as CustomEvent).detail ?? "test render error"));
    window.addEventListener(RENDER_ERROR_EVENT, onEvent);
    return () => window.removeEventListener(RENDER_ERROR_EVENT, onEvent);
  }, []);
  if (message != null) throw new Error(message);
  return null;
}
