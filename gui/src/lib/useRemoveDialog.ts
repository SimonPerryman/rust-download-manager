import { useCallback, useEffect, useMemo, useState, type RefObject } from "react";
import * as api from "./api";
import { toRemove } from "./archive";
import { removedManyMessage, removedMessage } from "./copy";
import { focusIsFree } from "./focus";
import { loadConfirmRemove, saveConfirmRemove } from "./prefs";
import type { DownloadSnapshot } from "./types";

type Deps = {
  byId: (id: number) => DownloadSnapshot | undefined;
  seeded: boolean;
  notify: (message: string) => void;
  fail: (e: unknown) => void;
  tableRef: RefObject<HTMLTableElement | null>;
};

/**
 * Removing archived rows for good: the Remove dialog, and whether to ask
 * first at all.
 */
export function useRemoveDialog({ byId, seeded, notify, fail, tableRef }: Deps) {
  const [removeIds, setRemoveIds] = useState<number[] | null>(null);
  const [confirmRemove, setConfirmRemove] = useState(loadConfirmRemove);
  const removeNow = useCallback(
    (rows: readonly DownloadSnapshot[]) => {
      if (!rows.length) return;
      Promise.all(rows.map((d) => api.remove(d.id)))
        .then(() => notify(rows.length === 1 ? removedMessage(rows[0]) : removedManyMessage(rows)))
        .catch(fail);
    },
    [notify, fail],
  );
  const askRemove = useCallback(
    (rows: DownloadSnapshot[]) => {
      // Only archived, finished rows go, with or without the dialog.
      const targets = toRemove(rows);
      if (!targets.length) return;
      if (confirmRemove) setRemoveIds(targets.map((d) => d.id));
      else removeNow(targets);
    },
    [confirmRemove, removeNow],
  );
  const updateConfirmRemove = useCallback((ask: boolean) => {
    setConfirmRemove(ask);
    saveConfirmRemove(ask);
  }, []);
  // The dialog follows the live rows and closes itself once none can be removed
  // (a row resumed, or unarchived by the archive toast's Undo, while it was open).
  const removeRows = useMemo(
    () => toRemove((removeIds ?? []).flatMap((id) => byId(id) ?? [])),
    [removeIds, byId],
  );
  const removeOpen = removeRows.length > 0;
  const closeRemove = useCallback(() => {
    setRemoveIds(null);
    setTimeout(() => focusIsFree() && tableRef.current?.focus({ preventScroll: true }));
  }, []);
  useEffect(() => {
    if (removeIds != null && seeded && !removeOpen) closeRemove();
  }, [removeIds, seeded, removeOpen, closeRemove]);
  const confirmRemoveRows = (dontAsk: boolean) => {
    setRemoveIds(null);
    if (dontAsk) updateConfirmRemove(false);
    removeNow(removeRows);
    setTimeout(() => focusIsFree() && tableRef.current?.focus({ preventScroll: true }));
  };

  return {
    removeIds,
    askRemove,
    confirmRemove,
    updateConfirmRemove,
    removeRows,
    removeOpen,
    closeRemove,
    confirmRemoveRows,
  };
}
