import { useEffect, useState } from "react";
import * as api from "./api";
import { readLogWithEvents } from "./history";
import { mergeLog, pollLog } from "./log";
import type { LogLine } from "./types";

/**
 * Download `id`'s recent log lines, with its history events among them, polled from the engine while the caller is
 * mounted (the Log tab mounts only while it's open), so polling stops when the
 * tab changes or the pane closes. `null` until the first read arrives.
 */
export function useDownloadLog(id: number): LogLine[] | null {
  const [lines, setLines] = useState<LogLine[] | null>(null);

  useEffect(() => {
    setLines(null);
    const read = (n: number) => readLogWithEvents(n, api.downloadLog, api.downloadHistory);
    return pollLog(id, read, (next) => setLines((prev) => mergeLog(prev, next)));
  }, [id]);

  return lines;
}
