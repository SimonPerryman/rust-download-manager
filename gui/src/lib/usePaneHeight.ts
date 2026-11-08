import { useCallback, useEffect, useState } from "react";
import { clampPaneHeight, PANE_DEFAULT_H } from "./details";
import { loadPaneHeight, savePaneHeight } from "./prefs";

const viewport = () => window.innerHeight;

/**
 * The details pane's height: restored from local prefs, saved as it changes,
 * and kept clear of the table as the window resizes.
 */
export function usePaneHeight(): {
  height: number;
  maxHeight: number;
  resize: (px: number) => void;
} {
  const [wanted, setWanted] = useState(() => loadPaneHeight() ?? PANE_DEFAULT_H);
  const [vh, setVh] = useState(viewport);

  useEffect(() => {
    const onResize = () => setVh(viewport());
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  useEffect(() => {
    savePaneHeight(wanted);
  }, [wanted]);

  const resize = useCallback((px: number) => setWanted(clampPaneHeight(px, viewport())), []);
  return {
    height: clampPaneHeight(wanted, vh),
    maxHeight: clampPaneHeight(Infinity, vh),
    resize,
  };
}
