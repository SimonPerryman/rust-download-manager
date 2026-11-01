import { useCallback, useLayoutEffect, useState } from "react";
import { applyPrefs, loadPrefs, savePrefs, type Prefs } from "./prefs";

const darkQuery = () => window.matchMedia("(prefers-color-scheme: dark)");

/** Theme + density prefs: loaded from storage, applied to <html>, saved on change. */
export function usePrefs(): [Prefs, (patch: Partial<Prefs>) => void] {
  const [prefs, setPrefs] = useState(loadPrefs);

  useLayoutEffect(() => {
    const mq = darkQuery();
    const apply = () => applyPrefs(prefs, document.documentElement, mq.matches);
    apply();
    // Only "system" needs to track live OS theme changes.
    if (prefs.theme !== "system") return;
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [prefs]);

  const update = useCallback((patch: Partial<Prefs>) => {
    setPrefs((prev) => {
      const next = { ...prev, ...patch };
      savePrefs(next);
      return next;
    });
  }, []);

  return [prefs, update];
}
