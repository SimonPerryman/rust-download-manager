import { useCallback, useEffect, useMemo, useState, type RefObject } from "react";
import * as api from "./api";
import { stoppedManyMessage, stoppedMessage } from "./copy";
import { focusIsFree } from "./focus";
import { isStoppable } from "./rowView";
import type { DownloadSnapshot } from "./types";

// How long a confirmed stop may take to land before focus gives up on the row.
const REFOCUS_MS = 2_000;

type Deps = {
  byId: (id: number) => DownloadSnapshot | undefined;
  notify: (message: string) => void;
  fail: (e: unknown) => void;
  tableRef: RefObject<HTMLTableElement | null>;
};

/**
 * Stopping downloads: the Stop dialog, and where focus goes once a stop
 * lands or the dialog closes.
 */
export function useStopDialog({ byId, notify, fail, tableRef }: Deps) {
  const [stopIds, setStopIds] = useState<number[] | null>(null);
  const [refocusId, setRefocusId] = useState<number | null>(null);

  // The dialog follows the live rows and closes itself once none can be stopped.
  const stopRows = useMemo(() => (stopIds ?? []).map(byId).filter(isStoppable), [stopIds, byId]);
  const stopOpen = stopRows.length > 0;
  useEffect(() => {
    if (stopIds == null || stopOpen) return;
    const id = stopIds.length === 1 ? stopIds[0] : null;
    setStopIds(null);
    // The rows finished or failed under the dialog: give focus back to the row (or the table).
    setTimeout(() => {
      if (!focusIsFree()) return;
      const button =
        id != null
          ? document.querySelector<HTMLButtonElement>(
              `[data-testid="download-row"][data-id="${id}"] .c-act button`,
            )
          : null;
      (button ?? tableRef.current)?.focus();
    });
  }, [stopIds, stopOpen]);

  const cancelStop = useCallback(() => {
    const ids = stopIds;
    setStopIds(null);
    if (ids == null) return;
    setTimeout(() => {
      const button =
        ids.length === 1
          ? document.querySelector<HTMLButtonElement>(
              `[data-testid="download-row"][data-id="${ids[0]}"] button[aria-label="Stop"]`,
            )
          : null;
      (button ?? tableRef.current)?.focus();
    });
  }, [stopIds]);

  const confirmStop = (keep: boolean) => {
    const rows = stopRows;
    setStopIds(null);
    if (rows.length === 0) return;
    Promise.all(rows.map((d) => api.stop(d.id, keep)))
      .then(() => {
        if (rows.length === 1) {
          notify(stoppedMessage(rows[0], keep));
          setRefocusId(rows[0].id);
        } else {
          notify(stoppedManyMessage(rows.length, keep));
          setTimeout(() => focusIsFree() && tableRef.current?.focus());
        }
      })
      .catch(fail);
  };

  // If the stopped row never shows up as Stopped here (filtered out, say), focus the table.
  useEffect(() => {
    if (refocusId == null) return;
    const t = setTimeout(() => {
      if (focusIsFree()) tableRef.current?.focus();
      setRefocusId(null);
    }, REFOCUS_MS);
    return () => clearTimeout(t);
  }, [refocusId]);
  const onRefocused = useCallback(() => setRefocusId(null), []);

  return {
    stopIds,
    setStopIds,
    stopRows,
    stopOpen,
    cancelStop,
    confirmStop,
    refocusId,
    onRefocused,
  };
}
