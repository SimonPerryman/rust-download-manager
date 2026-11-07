import { useEffect, useRef, useState, type RefObject } from "react";
import * as api from "./api";
import { NO_LIMIT, composeRate, rateDraft, sameRate, type RateDraft } from "./rate";
import type { DownloadSnapshot } from "./types";

// A speed cap only matters while a download can still stream, so finished
// rows neither show nor edit it.
export const LIMIT_SHOWN: DownloadSnapshot["status"][] = ["downloading", "paused", "queued"];

/** The row's inline speed-limit editor; `draft` is null while it's closed. */
export type LimitEditor = {
  draft: RateDraft | null;
  error?: string;
  /** The global cap, read when the editor opens, for the "won't go faster" note. */
  globalLimit: number | null;
  open: () => void;
  change: (d: RateDraft) => void;
  /** `refocus` returns focus to the gauge button (keyboard commits), not on a click-away. */
  commit: (next?: RateDraft, refocus?: boolean) => void;
  cancel: () => void;
  /** The gauge button, where focus goes back when the editor closes from the keyboard. */
  buttonRef: RefObject<HTMLButtonElement>;
};

// Commits on Enter or when focus leaves the editor; the engine validates and
// the row redraws from its snapshot. An edit still open when the row goes
// away is flushed, best effort.
export function useLimitEditor(d: DownloadSnapshot): LimitEditor {
  const [draft, setDraft] = useState<RateDraft | null>(null);
  const [error, setError] = useState<string>();
  const [globalLimit, setGlobalLimit] = useState<number | null>(null);
  const pending = useRef(false);
  // A commit asked for while one was in flight (e.g. a click-away), replayed after it.
  const deferred = useRef<{ refocus: boolean } | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const latest = useRef({ draft, d });
  latest.current = { draft, d };

  const editable = LIMIT_SHOWN.includes(d.status);
  useEffect(() => {
    if (!editable) {
      setDraft(null);
      setError(undefined);
    }
  }, [editable]);

  useEffect(
    () => () => {
      const { draft, d } = latest.current;
      if (!draft || !LIMIT_SHOWN.includes(d.status) || sameRate(draft, rateDraft(d.limit))) return;
      const r = composeRate(draft);
      if (r.ok) api.setLimit(d.id, r.rate || null).catch(() => {});
    },
    [],
  );

  const close = (refocus = false) => {
    setDraft(null);
    setError(undefined);
    // After the editor unmounts, so focus isn't dropped on <body> — but never
    // taken back from somewhere the user has moved it in the meantime.
    if (refocus) {
      setTimeout(() => {
        const active = document.activeElement;
        if (!active || active === document.body) buttonRef.current?.focus();
      });
    }
  };

  const commit = async (next?: RateDraft, refocus = false) => {
    // `latest` rather than this render's `draft`: a blur fired as the editor
    // closes must not re-send a draft that was just committed or cancelled.
    const { draft, d } = latest.current;
    if (!draft) return;
    if (pending.current) {
      deferred.current = { refocus };
      return;
    }
    const value = next ?? draft;
    if (sameRate(value, rateDraft(d.limit))) return close(refocus);
    const r = composeRate(value);
    if (!r.ok) return setError(r.error);
    pending.current = true;
    try {
      await api.setLimit(d.id, r.rate || null);
      // Anything typed while this was in flight stays open for its own commit.
      if (sameRate(latest.current.draft ?? NO_LIMIT, value)) close(refocus);
    } catch (e) {
      setError(String(e));
    } finally {
      pending.current = false;
    }
    const again = deferred.current;
    deferred.current = null;
    // Replays only a draft newer than the one just sent (a no-op once closed).
    if (again && !sameRate(latest.current.draft ?? NO_LIMIT, value)) {
      await commit(undefined, again.refocus);
    }
  };

  return {
    draft,
    error,
    globalLimit,
    open: () => {
      setDraft(rateDraft(d.limit));
      setError(undefined);
      api
        .getSettings()
        .then((s) => setGlobalLimit(s.global_limit))
        .catch(() => setGlobalLimit(null));
    },
    change: (next) => {
      setDraft(next);
      setError(undefined);
    },
    commit,
    cancel: () => close(true),
    buttonRef,
  };
}
