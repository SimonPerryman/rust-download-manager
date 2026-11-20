import { useEffect, useMemo, useRef, useState } from "react";
import * as api from "../../lib/api";
import { dayGroups, eventTime, kindLabel } from "../../lib/history";
import type { HistoryEvent } from "../../lib/types";
import { trapTab } from "../dialogs/trapTab";

type Props = {
  ruleId: number;
  name: string;
  onClose: () => void;
};

// What a rule did, grouped by day, newest first: time, type and text, as the Log tab shows them.
export function RuleHistoryDialog({ ruleId, name, onClose }: Props) {
  const [events, setEvents] = useState<HistoryEvent[] | null>(null);
  const dialog = useRef<HTMLDivElement>(null);
  const close = useRef<HTMLButtonElement>(null);
  const groups = useMemo(() => dayGroups(events ?? []), [events]);

  useEffect(() => {
    let live = true;
    api.ruleHistory(ruleId).then(
      (found) => live && setEvents(found),
      () => live && setEvents([]),
    );
    return () => {
      live = false;
    };
  }, [ruleId]);

  useEffect(() => {
    close.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      if (e.key === "Tab") trapTab(e, dialog.current);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-60 grid place-items-center bg-(--backdrop) p-4"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby="rule-history-title"
        data-testid="rule-history-dialog"
        className="flex max-h-[80vh] w-full max-w-[520px] flex-col rounded-lg bg-raised text-ink shadow-dialog"
      >
        <div className="flex items-baseline gap-3 px-6 pt-6">
          <h2
            id="rule-history-title"
            className="min-w-0 truncate text-xl font-semibold tracking-tight"
          >
            {name}: history
          </h2>
          <span className="ml-auto shrink-0 text-xs text-g400">Last 90 days</span>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
          {events !== null && groups.length === 0 && (
            <p data-testid="rule-history-empty" className="text-g600">
              Nothing yet
            </p>
          )}
          {groups.map((g) => (
            <section key={g.heading} data-testid="history-day" className="mb-3 last:mb-0">
              <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-g400">
                {g.heading}
              </h3>
              <ol className="grid gap-1 text-sm">
                {g.events.map((e, i) => (
                  <li
                    key={i}
                    data-testid="history-row"
                    className="grid grid-cols-[4.5rem_4.5rem_minmax(0,1fr)] gap-3"
                  >
                    <time className="text-g400 tabular-nums">{eventTime(e.at)}</time>
                    <span className="text-g600">{kindLabel(e.kind)}</span>
                    <span className="[overflow-wrap:anywhere]">{e.details}</span>
                  </li>
                ))}
              </ol>
            </section>
          ))}
        </div>
        <div className="flex items-center justify-end rounded-b-lg bg-surface px-6 py-4">
          <button ref={close} className="btn btn-ghost" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
