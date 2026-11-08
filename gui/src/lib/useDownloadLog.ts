import { useEffect, useState } from "react";
import * as api from "./api";
import { mergeLog, pollLog } from "./log";
import type { LogLine } from "./types";

/**
 * Download `id`'s recent log lines, polled from the engine while the caller is
 * mounted (the Log tab mounts only while it's open), so polling stops when the
 * tab changes or the pane closes. `null` until the first read arrives.
 */
export function useDownloadLog(id: number): LogLine[] | null {
  const [lines, setLines] = useState<LogLine[] | null>(null);

  useEffect(() => {
    setLines(null);
    return pollLog(id, api.downloadLog, (next) => setLines((prev) => mergeLog(prev, next)));
  }, [id]);

  return lines;
}
