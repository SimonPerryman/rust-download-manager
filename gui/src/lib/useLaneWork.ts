import { useMemo, useRef } from "react";
import { advanceAll, type LaneWork } from "./laneWork";
import type { DownloadSnapshot } from "./types";

/**
 * What each download's lanes have written, folded in from every snapshot as it
 * arrives (whether or not its pane is open), so the lane rows can show each
 * lane's own work. Computed in render, not an effect, so the rows never lag
 * the snapshot they're drawn with; folding the same snapshot twice is a no-op.
 */
export function useLaneWork(downloads: DownloadSnapshot[]): ReadonlyMap<number, LaneWork> {
  const prev = useRef<ReadonlyMap<number, LaneWork>>(new Map());
  return useMemo(() => {
    const next = advanceAll(prev.current, downloads, Date.now());
    prev.current = next;
    return next;
  }, [downloads]);
}
