import { useCallback, useEffect, useState } from "react";

/** How long a Copy button says "Copied" before it goes back to its label. */
export const COPIED_MS = 1_500;

/**
 * A short-lived confirmation, such as Copy's "Copied": the value passed to
 * `mark` (e.g. how many lines went), held for `COPIED_MS`, then `null`
 * again. Every call restarts the time, even with the same value.
 */
export function useCopied<T>(): [T | null, (value: T) => void] {
  // Wrapped in a fresh object per call, so the same value restarts the time.
  const [mark, setMark] = useState<{ value: T } | null>(null);
  useEffect(() => {
    if (mark == null) return;
    const t = setTimeout(() => setMark(null), COPIED_MS);
    return () => clearTimeout(t);
  }, [mark]);
  const set = useCallback((value: T) => setMark({ value }), []);
  return [mark?.value ?? null, set];
}
