import { useCallback, useMemo, useState } from "react";
import * as api from "./api";
import { startGroups, type StartGroup } from "./startAt";
import type { DownloadSnapshot } from "./types";

/** What the Start at dialog is for, and what to do with the time it returns. */
export interface TimeDialog {
  title: string;
  button: string;
  apply: (at: string) => Promise<unknown>;
}

/**
 * The start-at schedules the downloads are in and what can be done with them: the Start at
 * dialog (a row's Start at…, a schedule's Edit, Reschedule's Pick a time…), Start now, and
 * Dismiss. The engine holds and starts the downloads; these only ask it to.
 */
export function useStartActions(
  downloads: readonly DownloadSnapshot[],
  fail: (e: unknown) => void,
) {
  const [timeDialog, setTimeDialog] = useState<TimeDialog | null>(null);
  const groups = useMemo(() => startGroups(downloads), [downloads]);
  const missed = useMemo(() => groups.filter((g) => g.missed), [groups]);
  const upcoming = useMemo(() => groups.filter((g) => !g.missed), [groups]);
  const numbers = (gs: readonly StartGroup[]) => gs.map((g) => g.group);

  const askStartAt = useCallback((rows: number[]) => {
    setTimeDialog({
      title: "Start at",
      button: "Schedule",
      apply: (at) => api.scheduleStart(rows, at),
    });
  }, []);
  const askReschedule = useCallback((gs: readonly StartGroup[], title = "Reschedule") => {
    const which = numbers(gs);
    setTimeDialog({ title, button: "Reschedule", apply: (at) => api.reschedule(which, at) });
  }, []);
  const rescheduleTo = useCallback(
    (gs: readonly StartGroup[], at: string) => {
      api.reschedule(numbers(gs), at).catch(fail);
    },
    [fail],
  );
  const startNow = useCallback(
    (gs: readonly StartGroup[]) => api.startSchedulesNow(numbers(gs)).catch(fail),
    [fail],
  );
  const dismiss = useCallback(
    (gs: readonly StartGroup[]) => api.dismissSchedules(numbers(gs)).catch(fail),
    [fail],
  );

  return {
    groups,
    missed,
    upcoming,
    timeDialog,
    closeTimeDialog: useCallback(() => setTimeDialog(null), []),
    askStartAt,
    askReschedule,
    rescheduleTo,
    startNow,
    dismiss,
  };
}
