import { useCallback, useEffect, useMemo, useReducer, useRef } from "react";
import * as api from "./api";
import { relinkedFilesMessage, relinkedMessage } from "./copy";
import { findReducer, initialFindState, missingIds, type FindState } from "./find";
import type { DownloadSnapshot, FoundChoice } from "./types";

export interface Find {
  state: FindState;
  /** Find file: search for one missing download. */
  start: (id: number) => void;
  /** Find files: one search for these missing downloads (all of them, by default). */
  startMany: (ids?: number[]) => void;
  cancel: () => void;
  /** Forget a record's result (Skip). */
  skip: (id: number) => void;
  closeReview: () => void;
  /** Relink the accepted candidates; resolves once they are done. */
  relink: (choices: FoundChoice[]) => Promise<void>;
}

/**
 * Follows the engine's Find file events and keeps their results in memory. Starting, cancelling
 * and relinking are engine commands; this decides nothing about a match.
 */
export function useFind(
  downloads: readonly DownloadSnapshot[],
  notify: (message: string) => void,
  fail: (e: unknown) => void,
): Find {
  const [state, dispatch] = useReducer(findReducer, initialFindState);
  const stateRef = useRef(state);
  stateRef.current = state;
  const pending = useRef<Promise<number> | null>(null);
  const downloadsRef = useRef(downloads);
  downloadsRef.current = downloads;

  useEffect(() => {
    let off: (() => void) | null = null;
    let gone = false;
    api
      .onFindEvent((event) => {
        dispatch({ type: "event", event });
        if (event.kind !== "finished") return;
        // A slow window can miss results; the engine's kept ones are the source of truth.
        api
          .findResults()
          .catch(() => [])
          .then((results) => dispatch({ type: "synced", results, finished: event }));
      })
      .then((un) => {
        if (gone) un();
        else off = un;
      })
      .catch(fail);
    // A reloaded window picks up what the engine kept.
    api
      .findResults()
      .then((results) => dispatch({ type: "synced", results, finished: null }))
      .catch(() => {});
    return () => {
      gone = true;
      off?.();
    };
  }, [fail]);

  // Results go once their download is relinked, removed or its file reappears.
  const missingKey = missingIds(downloads).join(",");
  useEffect(() => {
    const missing = new Set(missingKey ? missingKey.split(",").map(Number) : []);
    dispatch({ type: "prune", missing });
  }, [missingKey]);

  const run = useCallback(
    (ids: number[], many: boolean, command: () => Promise<number>) => {
      dispatch({ type: "begin", ids, many });
      const started = command();
      pending.current = started;
      started
        .then((search) => dispatch({ type: "attach", search }))
        .catch((e) => {
          dispatch({ type: "failed" });
          fail(e);
        });
    },
    [fail],
  );

  const start = useCallback((id: number) => run([id], false, () => api.findFile(id)), [run]);
  const startMany = useCallback(
    (ids?: number[]) => run(ids ?? missingIds(downloadsRef.current), true, api.findFiles),
    [run],
  );
  const cancel = useCallback(() => {
    const started = pending.current;
    if (started) started.then((search) => api.cancelFind(search)).catch(fail);
  }, [fail]);
  const skip = useCallback((id: number) => dispatch({ type: "skip", id }), []);
  const closeReview = useCallback(() => dispatch({ type: "closeReview" }), []);

  const relink = useCallback(
    async (choices: FoundChoice[]) => {
      if (choices.length === 0) return;
      try {
        const n = await api.relinkFound(choices);
        const one = choices.length === 1 ? choices[0] : null;
        const old = one && downloadsRef.current.find((d) => d.id === one.id);
        notify(
          n === 1 && one && old
            ? relinkedMessage(old.final_path ?? old.filename, one.path)
            : relinkedFilesMessage(n),
        );
      } catch (e) {
        fail(e);
      }
    },
    [notify, fail],
  );

  return useMemo(
    () => ({ state, start, startMany, cancel, skip, closeReview, relink }),
    [state, start, startMany, cancel, skip, closeReview, relink],
  );
}
