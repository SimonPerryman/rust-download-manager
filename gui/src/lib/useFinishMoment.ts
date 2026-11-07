import { useCallback, useEffect, useRef, useState } from "react";
import {
  FRESH_MS,
  MAX_NOTIFICATIONS,
  pruneUnopened,
  trackFinishes,
  type StatusMemo,
} from "./finish";
import { loadUnopened, saveUnopened } from "./prefs";
import type { DownloadSnapshot } from "./types";

/** A finish notification: the download as it was when it completed. */
export interface FinishNotice {
  id: number;
  download: DownloadSnapshot;
}

export interface FinishMoment {
  /** Rows in their fresh tint: id → when it started (ms since epoch), for the CSS delay. */
  fresh: ReadonlyMap<number, number>;
  /** Completed rows not yet opened (persisted); they linger with a solid Open. */
  unopened: ReadonlySet<number>;
  /** Newest last, at most `MAX_NOTIFICATIONS`. */
  notifications: FinishNotice[];
  /** Bumps on every finish this session (0 until then), to restart the Done count's bump. */
  bumpSeq: number;
  /** Call after a successful open. */
  markOpened: (id: number) => void;
  dismissNotification: (id: number) => void;
}

const sameSet = (a: ReadonlySet<number>, b: ReadonlySet<number>) =>
  a.size === b.size && [...a].every((id) => b.has(id));

/**
 * The finish moment for downloads that complete while the app is watching:
 * fresh tint, unopened linger, notification and Done bump. Rows already
 * completed in the first payload get none of it. Feed it the raw downloads.
 */
export function useFinishMoment(downloads: DownloadSnapshot[], seeded: boolean): FinishMoment {
  const memo = useRef<StatusMemo | null>(null);
  const [unopened, setUnopened] = useState<ReadonlySet<number>>(loadUnopened);
  const unopenedRef = useRef(unopened);
  const [fresh, setFresh] = useState<ReadonlyMap<number, number>>(new Map());
  const [notifications, setNotifications] = useState<FinishNotice[]>([]);
  const [bumpSeq, setBumpSeq] = useState(0);
  const timers = useRef(new Map<number, ReturnType<typeof setTimeout>>());

  const storeUnopened = useCallback((next: ReadonlySet<number>) => {
    unopenedRef.current = next;
    setUnopened(next);
    saveUnopened(next);
  }, []);

  useEffect(() => {
    // The empty pre-load render must not seed the memo, or every old completed
    // row would look newly finished on the first real payload.
    if (!seeded) return;
    const r = trackFinishes(memo.current, downloads);
    memo.current = r.memo;
    let next: ReadonlySet<number> = unopenedRef.current;

    if (r.finished.length > 0) {
      const now = Date.now();
      next = new Set([...next, ...r.finished]);
      for (const id of r.finished) {
        clearTimeout(timers.current.get(id));
        timers.current.set(
          id,
          setTimeout(() => {
            timers.current.delete(id);
            setFresh((prev) => {
              const m = new Map(prev);
              m.delete(id);
              return m;
            });
          }, FRESH_MS),
        );
      }
      setFresh((prev) => {
        const m = new Map(prev);
        for (const id of r.finished) m.set(id, now);
        return m;
      });
      const byId = new Map(downloads.map((d) => [d.id, d]));
      const added = r.finished.map((id) => ({ id, download: byId.get(id)! }));
      setNotifications((prev) =>
        [...prev.filter((n) => !r.finished.includes(n.id)), ...added].slice(-MAX_NOTIFICATIONS),
      );
      setBumpSeq((n) => n + 1);
    }

    const pruned = pruneUnopened(next, downloads);
    if (!sameSet(pruned, unopenedRef.current)) storeUnopened(pruned);
  }, [downloads, seeded, storeUnopened]);

  useEffect(() => {
    const pending = timers.current;
    return () => {
      for (const t of pending.values()) clearTimeout(t);
      pending.clear();
    };
  }, []);

  const markOpened = useCallback(
    (id: number) => {
      if (!unopenedRef.current.has(id)) return;
      const next = new Set(unopenedRef.current);
      next.delete(id);
      storeUnopened(next);
    },
    [storeUnopened],
  );

  const dismissNotification = useCallback((id: number) => {
    setNotifications((prev) => prev.filter((n) => n.id !== id));
  }, []);

  return { fresh, unopened, notifications, bumpSeq, markOpened, dismissNotification };
}
