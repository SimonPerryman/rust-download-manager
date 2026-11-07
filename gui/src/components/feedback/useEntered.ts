import { useCallback, useEffect, useState } from "react";

const reducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

/**
 * Whether an overlay card's entry animation is over (`data-entered`), so the
 * e2e never clicks a card still sliding in. `animationend` alone isn't enough:
 * WebView2 paints no frames for an occluded window, so a timer of the entry
 * length + 100 ms backs it up, and reduced motion counts as entered at once.
 */
export function useEntered(entryMs: number): [boolean, () => void] {
  const [entered, setEntered] = useState(reducedMotion);

  useEffect(() => {
    if (entered) return;
    const t = setTimeout(() => setEntered(true), entryMs + 100);
    return () => clearTimeout(t);
  }, [entered, entryMs]);

  const onAnimationEnd = useCallback(() => setEntered(true), []);
  return [entered, onAnimationEnd];
}
