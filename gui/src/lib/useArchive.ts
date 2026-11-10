import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "./api";
import {
  ARCHIVE_STATUSES,
  archivedMessage,
  isArchivable,
  restoredMessage,
  UNDO_MS,
  type UndoBatch,
} from "./archive";
import { nothingRestored } from "./copy";
import type { DownloadSnapshot } from "./types";

/** Archive the archivable rows among `rows`; resolves with the ids the engine archived. */
export async function archiveRows(rows: readonly DownloadSnapshot[]): Promise<number[]> {
  const ids = rows.filter(isArchivable).map((d) => d.id);
  return ids.length === 0 ? [] : api.archive(ids);
}

/** Unarchive `batch`; resolves with the toast saying how many actually came back. */
export async function undoArchive(batch: UndoBatch): Promise<string> {
  const restored = await api.unarchive(batch.ids);
  return restored === 0 ? nothingRestored : restoredMessage(restored);
}

/**
 * Archive with an `UNDO_MS` undo toast: the rows are archived at once, and
 * Undo unarchives exactly the ids the engine archived. `notify` shows an info
 * toast (after Undo, or when a call fails).
 */
export function useArchive(notify: (message: string) => void): {
  /** The archive the undo toast offers to undo. */
  pending: UndoBatch | null;
  /** Archive finished: done and stopped rows among `rows`. */
  archiveFinished: (rows: readonly DownloadSnapshot[]) => void;
  /** Archive the finished rows among `rows`. */
  archive: (rows: readonly DownloadSnapshot[]) => void;
  undo: () => void;
} {
  const [pending, setPending] = useState<UndoBatch | null>(null);
  const pendingRef = useRef<UndoBatch | null>(null);
  const seq = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const notifyRef = useRef(notify);
  notifyRef.current = notify;

  const show = useCallback((batch: UndoBatch | null) => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    pendingRef.current = batch;
    setPending(batch);
    if (batch) timer.current = setTimeout(() => show(null), UNDO_MS);
  }, []);

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const offerUndo = useCallback(
    (ids: number[], rows: readonly DownloadSnapshot[]) => {
      if (ids.length === 0) return;
      const archived = rows.filter((d) => ids.includes(d.id));
      seq.current += 1;
      show({ seq: seq.current, ids, message: archivedMessage(archived) });
    },
    [show],
  );

  const archiveFinished = useCallback(
    (rows: readonly DownloadSnapshot[]) => {
      const ids = rows.filter((d) => ARCHIVE_STATUSES.includes(d.status)).map((d) => d.id);
      if (ids.length === 0) return;
      api
        .archiveFinished(ARCHIVE_STATUSES, ids)
        .then((archived) => offerUndo(archived, rows))
        .catch((e) => notifyRef.current(`Couldn't archive. ${e}`));
    },
    [offerUndo],
  );

  const archive = useCallback(
    (rows: readonly DownloadSnapshot[]) => {
      archiveRows(rows)
        .then((archived) => offerUndo(archived, rows))
        .catch((e) => notifyRef.current(`Couldn't archive. ${e}`));
    },
    [offerUndo],
  );

  const undo = useCallback(() => {
    const batch = pendingRef.current;
    if (!batch) return;
    show(null);
    undoArchive(batch)
      .then((message) => notifyRef.current(message))
      .catch((e) => notifyRef.current(`Couldn't undo. ${e}`));
  }, [show]);

  return { pending, archiveFinished, archive, undo };
}
