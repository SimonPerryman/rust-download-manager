import { useCallback, useEffect, useLayoutEffect, useState } from "react";
import { applyPrefs, loadPrefs, resolveTheme, savePrefs, type Prefs } from "./prefs";

const darkQuery = () => window.matchMedia("(prefers-color-scheme: dark)");

/**
 * Theme + density prefs: loaded from storage, applied to <html>, saved on
 * change. The third item is the theme actually showing ("system" resolved
 * against the OS, tracked live).
 */
export function usePrefs(): [Prefs, (patch: Partial<Prefs>) => void, "light" | "dark"] {
  const [prefs, setPrefs] = useState(loadPrefs);
  const [systemDark, setSystemDark] = useState(() => darkQuery().matches);

  useEffect(() => {
    const mq = darkQuery();
    const track = () => setSystemDark(mq.matches);
    track();
    mq.addEventListener("change", track);
    return () => mq.removeEventListener("change", track);
  }, []);

  useLayoutEffect(() => {
    applyPrefs(prefs, document.documentElement, systemDark);
  }, [prefs, systemDark]);

  const update = useCallback((patch: Partial<Prefs>) => {
    setPrefs((prev) => {
      const next = { ...prev, ...patch };
      savePrefs(next);
      return next;
    });
  }, []);

  return [prefs, update, resolveTheme(prefs.theme, systemDark)];
}
