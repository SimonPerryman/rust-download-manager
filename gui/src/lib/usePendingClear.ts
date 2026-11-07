import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "./api";
import {
  CLEAR_STATUSES,
  INITIAL_CLEAR,
  reduceClear,
  restoredMessage,
  UNDO_MS,
  type Batch,
  type ClearEvent,
  type ClearState,
} from "./pendingClear";
import type { DownloadSnapshot } from "./types";

/**
 * Clear finished with a `UNDO_MS` undo window over `reduceClear`. `notify`
 * shows an info toast (after Undo, or when the clear fails). Rows in `hidden`
 * are to be left out of everything shown; `pending` drives the undo toast.
 */
export function usePendingClear(
  downloads: DownloadSnapshot[],
  notify: (message: string) => void,
): {
  hidden: ReadonlySet<number>;
  pending: Batch | null;
  /** Plans a batch from the rows currently shown. */
  clearFinished: (shown: DownloadSnapshot[]) => void;
  undo: () => void;
} {
  const ref = useRef<ClearState>(INITIAL_CLEAR);
  const [state, setState] = useState<ClearState>(INITIAL_CLEAR);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const notifyRef = useRef(notify);
  notifyRef.current = notify;

  // Effects run here, outside any state updater (StrictMode double-invokes those).
  const dispatch = useCallback(function dispatch(e: ClearEvent) {
    const r = reduceClear(ref.current, e);
    ref.current = r.state;
    setState(r.state);
    for (const effect of r.effects) {
      if (effect.type === "arm" || effect.type === "disarm") {
        if (timer.current) clearTimeout(timer.current);
        timer.current = null;
      }
      if (effect.type === "arm") {
        const { seq } = effect;
        timer.current = setTimeout(() => {
          timer.current = null;
          dispatch({ type: "timeout", seq });
        }, UNDO_MS);
      } else if (effect.type === "commit") {
        const { seq, ids } = effect.batch;
        api.clear(CLEAR_STATUSES, ids).catch((err) => {
          dispatch({ type: "failed", seq });
          notifyRef.current(`Couldn't clear the list. ${err}`);
        });
      }
    }
  }, []);

  useEffect(() => dispatch({ type: "snapshot", downloads }), [downloads, dispatch]);

  // Closing inside the window clears nothing: the rows come back next launch.
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const clearFinished = useCallback(
    (shown: DownloadSnapshot[]) => dispatch({ type: "click", downloads: shown }),
    [dispatch],
  );

  const undo = useCallback(() => {
    const pending = ref.current.pending;
    if (!pending) return;
    dispatch({ type: "undo" });
    notifyRef.current(restoredMessage(pending.ids.length));
  }, [dispatch]);

  return { hidden: state.hidden, pending: state.pending, clearFinished, undo };
}
