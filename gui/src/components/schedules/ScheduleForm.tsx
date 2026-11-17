import { useEffect, useRef, useState } from "react";
import { dayShort, daysKind, daysName, ordered, toggled, WEEKDAYS } from "../../lib/days";
import { sanitizeAmount } from "../../lib/rate";
import {
  draftAutoName,
  draftProblem,
  NEW_SPEED,
  NEW_WINDOW,
  summary,
  type RuleDraft,
  type RuleKind,
} from "../../lib/schedule";
import type { Weekday } from "../../lib/types";
import { X } from "../icons/X";
import { trapTab } from "../dialogs/trapTab";
import { Switch } from "./Switch";

type Props = {
  /** The starting values: the defaults for a new rule, or the rule being edited. */
  initial: RuleDraft;
  /** Editing keeps the rule; the button then reads Save. */
  editing: boolean;
  /** Saves the draft; rejects with why it was refused. */
  onSubmit: (draft: RuleDraft) => Promise<void>;
  onClose: () => void;
};

type Choice = "daily" | "weekdays" | "weekends" | "choose";

const SETS: Record<Exclude<Choice, "choose">, Weekday[]> = {
  daily: [...WEEKDAYS],
  weekdays: WEEKDAYS.slice(0, 5),
  weekends: WEEKDAYS.slice(5),
};

const FIELD =
  "flex h-8 w-full rounded-md border border-line-strong bg-raised px-3 text-sm transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-hidden";

// The New schedule form: one modal. Type picks a Download window or a Speed limit and the
// fields below follow it. An existing rule keeps its type: the two live in separate lists.
export function ScheduleForm({ initial, editing, onSubmit, onClose }: Props) {
  const [draft, setDraft] = useState(initial);
  // The chips show under Choose days, starting from the days already picked.
  const [choosing, setChoosing] = useState(daysKind(initial.days) === "chosen");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const dialog = useRef<HTMLDivElement>(null);
  const nameInput = useRef<HTMLInputElement>(null);
  const set = (patch: Partial<RuleDraft>) => setDraft((d) => ({ ...d, ...patch }));

  useEffect(() => nameInput.current?.focus(), []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onClose();
      if (e.key === "Tab") trapTab(e, dialog.current);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  const problem = draftProblem(draft);
  const choice: Choice = choosing ? "choose" : (daysKind(draft.days) as Exclude<Choice, "choose">);
  // Under Choose days the select reads the set itself ("Mon, Tue and Thu").
  const chooseLabel = choosing && draft.days.length > 0 ? daysName(draft.days) : "Choose days";

  // Switching type swaps in the other type's usual times and days, unless they were changed.
  const pickKind = (kind: RuleKind) => {
    const from = draft.kind === "speed" ? NEW_SPEED : NEW_WINDOW;
    const to = kind === "speed" ? NEW_SPEED : NEW_WINDOW;
    const untouched =
      draft.start === from.start &&
      draft.end === from.end &&
      draft.days.join() === from.days.join();
    setDraft({
      ...draft,
      kind,
      ...(untouched ? { start: to.start, end: to.end, days: [...to.days] } : {}),
    });
    setChoosing(false);
  };

  const pickChoice = (c: Choice) => {
    if (c === "choose") {
      setChoosing(true);
      return;
    }
    setChoosing(false);
    set({ days: SETS[c] });
  };

  const submit = async () => {
    if (busy || problem) return;
    setBusy(true);
    setError(null);
    try {
      await onSubmit({ ...draft, days: ordered(draft.days) });
    } catch (e) {
      setBusy(false);
      setError(String(e));
    }
  };

  return (
    <div className="fixed inset-0 z-50">
      <div onClick={() => !busy && onClose()} className="fixed inset-0 bg-(--backdrop)" />
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-label={editing ? "Edit schedule" : "New schedule"}
        className="fixed left-1/2 top-1/2 z-50 w-full max-w-[520px] -translate-x-1/2 -translate-y-1/2 px-4"
      >
        <div className="flex max-h-[calc(100vh-2rem)] flex-col rounded-lg bg-raised text-ink shadow-dialog">
          <div className="flex shrink-0 items-start justify-between px-6 pt-6">
            <h2 className="text-xl font-semibold tracking-tight">
              {editing ? "Edit schedule" : "New schedule"}
            </h2>
            <button
              type="button"
              aria-label="Close"
              onClick={onClose}
              className="icon-btn -mr-2 -mt-1"
            >
              <X size={18} />
            </button>
          </div>

          <div className="min-h-0 space-y-5 overflow-y-auto px-6 py-5">
            <Field label="Type" htmlFor="sched-type">
              <select
                id="sched-type"
                aria-label="Type"
                className={FIELD}
                value={draft.kind}
                disabled={editing}
                onChange={(e) => pickKind(e.target.value as RuleKind)}
              >
                <option value="window">Download window</option>
                <option value="speed">Speed limit</option>
              </select>
            </Field>

            <Field label="Name" htmlFor="sched-name">
              <input
                id="sched-name"
                ref={nameInput}
                type="text"
                aria-label="Name"
                value={draft.name}
                onChange={(e) => set({ name: e.target.value })}
                onKeyDown={(e) => e.key === "Enter" && void submit()}
                placeholder={draftAutoName(draft)}
                className={FIELD}
              />
            </Field>

            {draft.kind === "speed" && (
              <Field label="Limit" htmlFor="sched-limit">
                <div className="flex items-center gap-2">
                  <input
                    id="sched-limit"
                    type="text"
                    aria-label="Limit"
                    inputMode="decimal"
                    value={draft.limit}
                    onChange={(e) => set({ limit: sanitizeAmount(e.target.value) })}
                    onKeyDown={(e) => e.key === "Enter" && void submit()}
                    placeholder="5"
                    className={`${FIELD} w-24 tabular-nums`}
                  />
                  <select aria-label="Unit" className={`${FIELD} w-28`} defaultValue="M">
                    <option value="M">MB/s</option>
                  </select>
                </div>
              </Field>
            )}

            <Field label="Times">
              <div className="flex items-center gap-2">
                <input
                  type="text"
                  aria-label="Start time"
                  inputMode="numeric"
                  maxLength={5}
                  value={draft.start}
                  onChange={(e) => set({ start: e.target.value })}
                  onKeyDown={(e) => e.key === "Enter" && void submit()}
                  placeholder="22:00"
                  className={`${FIELD} w-24 tabular-nums`}
                />
                <span className="text-g600">to</span>
                <input
                  type="text"
                  aria-label="End time"
                  inputMode="numeric"
                  maxLength={5}
                  value={draft.end}
                  onChange={(e) => set({ end: e.target.value })}
                  onKeyDown={(e) => e.key === "Enter" && void submit()}
                  placeholder="08:00"
                  className={`${FIELD} w-24 tabular-nums`}
                />
              </div>
            </Field>

            <Field label="Days" htmlFor="sched-days">
              <select
                id="sched-days"
                aria-label="Days"
                value={choice}
                onChange={(e) => pickChoice(e.target.value as Choice)}
                className={FIELD}
              >
                <option value="daily">Daily</option>
                <option value="weekdays">Weekdays</option>
                <option value="weekends">Weekends</option>
                <option value="choose">{chooseLabel}</option>
              </select>
              {choosing && (
                <div data-testid="day-chips" className="mt-2 flex gap-1">
                  {WEEKDAYS.map((day) => (
                    <span key={day} className={day === "sat" ? "ml-3 flex-1" : "flex-1"}>
                      <button
                        type="button"
                        aria-pressed={draft.days.includes(day)}
                        data-day={day}
                        onClick={() => set({ days: toggled(draft.days, day) })}
                        className={`h-7 w-full rounded-md text-sm ${
                          draft.days.includes(day)
                            ? "bg-btn text-on-btn"
                            : "bg-hover text-g600 hover:text-ink"
                        }`}
                      >
                        {dayShort(day)}
                      </button>
                    </span>
                  ))}
                </div>
              )}
            </Field>

            {draft.kind === "window" && (
              <div className="flex items-center justify-between rounded-md border border-line-strong px-3 py-2.5 text-sm">
                <span>Finish downloads in progress</span>
                <Switch
                  checked={draft.finishRunning}
                  label="Finish downloads in progress"
                  testId="finish-running"
                  onChange={(on) => set({ finishRunning: on })}
                />
              </div>
            )}
            {error && <p className="text-xs text-danger-ink">{error}</p>}
          </div>

          <div className="flex shrink-0 items-center gap-2 rounded-b-lg bg-surface px-6 py-4">
            <p
              data-testid="schedule-summary"
              className={`min-w-0 flex-1 text-sm ${problem ? "text-danger-ink" : "text-g600"}`}
            >
              {problem ?? summary(draft)}
            </p>
            <button type="button" onClick={onClose} disabled={busy} className="btn btn-ghost">
              Cancel
            </button>
            <button
              type="button"
              data-testid="schedule-submit"
              onClick={submit}
              disabled={busy || problem !== null}
              className="btn btn-primary"
            >
              {editing ? "Save" : "Add"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function Field({
  label,
  htmlFor,
  children,
}: {
  label: string;
  htmlFor?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-1">
      <label htmlFor={htmlFor} className="block text-sm font-medium">
        {label}
      </label>
      {children}
    </div>
  );
}
