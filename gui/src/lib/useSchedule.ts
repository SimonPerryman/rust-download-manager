import { useCallback, useEffect, useState } from "react";
import * as api from "./api";
import type { QueueState, Schedule } from "./types";

/**
 * The saved schedule rules and whether the queue window is open, kept live: the backend
 * says when either changes. `save` replaces the rule set and rejects with why one was refused.
 */
export function useSchedule(): {
  schedule: Schedule | null;
  queue: QueueState | null;
  save: (next: Schedule) => Promise<void>;
} {
  const [schedule, setSchedule] = useState<Schedule | null>(null);
  const [queue, setQueue] = useState<QueueState | null>(null);

  useEffect(() => {
    let live = true;
    api
      .getSchedule()
      .then((s) => live && setSchedule(s))
      .catch(() => {});
    api
      .getQueueState()
      .then((q) => live && setQueue((prev) => prev ?? q))
      .catch(() => {});
    const unlisten = api.onQueueStateChanged((q) => live && setQueue(q));
    return () => {
      live = false;
      unlisten.then((fn) => fn());
    };
  }, []);

  const save = useCallback(async (next: Schedule) => {
    await api.setSchedule(next);
    setSchedule(next);
    // The event follows the save; this read makes the status right even if it is late.
    api
      .getQueueState()
      .then(setQueue)
      .catch(() => {});
  }, []);

  return { schedule, queue, save };
}
