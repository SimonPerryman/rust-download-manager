import { useEffect, useRef, useState } from "react";
import { EMPTY_HISTORY, pushSample, SAMPLE_MS, type SpeedHistory } from "./speedHistory";
import type { DownloadSnapshot } from "./types";

/**
 * The last minute of speed per download and in total, sampled from the latest
 * snapshot once a second (snapshots arrive at uneven rates, so a timer keeps
 * the graph's x axis honest).
 */
export function useSpeedHistory(downloads: DownloadSnapshot[]): SpeedHistory {
  const [history, setHistory] = useState<SpeedHistory>(EMPTY_HISTORY);
  const latest = useRef(downloads);
  latest.current = downloads;

  useEffect(() => {
    const t = setInterval(() => setHistory((h) => pushSample(h, latest.current)), SAMPLE_MS);
    return () => clearInterval(t);
  }, []);

  return history;
}
