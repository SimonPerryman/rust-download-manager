import { useEffect, useRef, useState } from "react";
import { dayShort, daysKind, daysName, ordered, toggled, WEEKDAYS } from "../../lib/days";
import { normalizeTime, timeLabel } from "../../lib/clock";
import { RATE_UNITS, sanitizeAmount, type RateUnit } from "../../lib/rate";
import {
  draftAutoName,
  draftProblem,
  draftSummary,
  NEW_SPEED,
  NEW_WINDOW,
  type RuleDraft,
  type RuleKind,
  type ScopeKind,
  withTimes,
} from "../../lib/schedule";
import type { Weekday } from "../../lib/types";
import { ChevronDown } from "../icons/ChevronDown";
import { X } from "../icons/X";
import { trapTab } from "../dialogs/trapTab";
import { Field, FIELD } from "./FormField";
import { Switch } from "./Switch";

type Props = {
  /** The starting values: the defaults for a new rule, or the rule being edited. */
  initial: RuleDraft;
  /** Editing keeps the rule; the button then reads Save. */
  editing: boolean;
  /** What a rule can be pointed at: the tags and batches the downloads have now. */
  choices?: ScopeChoices;
  /** Saves the draft; rejects with why it was refused. */
  onSubmit: (draft: RuleDraft) => Promise<void>;
  onClose: () => void;
};

/** The tags and batches a rule's Applies to can offer. */
export interface ScopeChoices {
  tags: string[];
  batches: { id: number; label: string }[];
}

const NO_CHOICES: ScopeChoices = { tags: [], batches: [] };

type Choice = "daily" | "weekdays" | "weekends" | "choose";

const SETS: Record<Exclude<Choice, "choose">, Weekday[]> = {
  daily: [...WEEKDAYS],
  weekdays: WEEKDAYS.slice(0, 5),
  weekends: WEEKDAYS.slice(5),
};

// The Limit and Times rows share these columns so their second boxes line up.
const ROW = "grid grid-cols-[6rem_2rem_6rem] items-center gap-2";

// A select with its own chevron, 12px in from the edge, instead of the native arrow.
function SelectField({ className = "", children, ...props }: React.ComponentProps<"select">) {
  return (
    <div className={`relative ${className}`}>
      <select {...props} className={`${FIELD} appearance-none pr-8`}>
        {children}
      </select>
      <ChevronDown
        size={14}
        className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-g600"
      />
    </div>
  );
}

// The New schedule form: one modal. Type picks a Download window or a Speed limit and the
// fields below follow it. An existing rule keeps its type: the two live in separate lists.
export function ScheduleForm({ initial, editing, choices = NO_CHOICES, onSubmit, onClose }: Props) {
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

  const pickScope = (kind: ScopeKind) => set({ scopeKind: kind, scopeText: "", scopeBatch: null });

  // A batch the rule names stays pickable after its downloads are gone.
  const batches =
    draft.scopeBatch && !choices.batches.some((b) => b.id === draft.scopeBatch?.id)
      ? [draft.scopeBatch, ...choices.batches]
      : choices.batches;

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
      await onSubmit({ ...withTimes(draft), days: ordered(draft.days) });
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
              <SelectField
                id="sched-type"
                aria-label="Type"
                value={draft.kind}
                disabled={editing}
                onChange={(e) => pickKind(e.target.value as RuleKind)}
              >
                <option value="window">Download window</option>
                <option value="speed">Speed limit</option>
              </SelectField>
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

            <Field label="Applies to" htmlFor="sched-scope">
              <div className="grid grid-cols-[10rem_1fr] items-center gap-2">
                <SelectField
                  id="sched-scope"
                  aria-label="Applies to"
                  value={draft.scopeKind}
                  onChange={(e) => pickScope(e.target.value as ScopeKind)}
                >
                  <option value="all">All downloads</option>
                  <option value="tag">A tag</option>
                  <option value="host">A host</option>
                  <option value="batch">A batch</option>
                </SelectField>
                {(draft.scopeKind === "tag" || draft.scopeKind === "host") && (
                  <>
                    <input
                      type="text"
                      aria-label={draft.scopeKind === "tag" ? "Tag" : "Host"}
                      list={draft.scopeKind === "tag" ? "sched-tags" : undefined}
                      value={draft.scopeText}
                      onChange={(e) => set({ scopeText: e.target.value })}
                      onKeyDown={(e) => e.key === "Enter" && void submit()}
                      placeholder={draft.scopeKind === "tag" ? "course-2" : "example.com"}
                      className={FIELD}
                    />
                    {draft.scopeKind === "tag" && (
                      <datalist id="sched-tags">
                        {choices.tags.map((t) => (
                          <option key={t} value={t} />
                        ))}
                      </datalist>
                    )}
                  </>
                )}
                {draft.scopeKind === "batch" && (
                  <SelectField
                    aria-label="Batch"
                    value={draft.scopeBatch?.id ?? ""}
                    onChange={(e) =>
                      set({
                        scopeBatch: batches.find((b) => b.id === Number(e.target.value)) ?? null,
                      })
                    }
                  >
                    <option value="">{batches.length === 0 ? "No batches" : "Pick a batch"}</option>
                    {batches.map((b) => (
                      <option key={b.id} value={b.id}>
                        {b.label}
                      </option>
                    ))}
                  </SelectField>
                )}
              </div>
            </Field>

            {draft.kind === "speed" && (
              <Field label="Limit" htmlFor="sched-limit">
                <div className={ROW}>
                  <input
                    id="sched-limit"
                    type="text"
                    aria-label="Limit"
                    inputMode="decimal"
                    value={draft.limit}
                    onChange={(e) => set({ limit: sanitizeAmount(e.target.value) })}
                    onKeyDown={(e) => e.key === "Enter" && void submit()}
                    placeholder="5"
                    className={`${FIELD} tabular-nums`}
                  />
                  <SelectField
                    aria-label="Unit"
                    value={draft.limitUnit}
                    onChange={(e) => set({ limitUnit: e.target.value as RateUnit })}
                    className="col-start-3"
                  >
                    {RATE_UNITS.map((u) => (
                      <option key={u.value} value={u.value}>
                        {u.label}
                      </option>
                    ))}
                  </SelectField>
                </div>
              </Field>
            )}

            <Field label="Times">
              <div className={ROW}>
                <input
                  type="text"
                  aria-label="Start time"
                  inputMode="numeric"
                  maxLength={8}
                  value={draft.start}
                  onChange={(e) => set({ start: e.target.value })}
                  onBlur={() => set({ start: normalizeTime(draft.start) ?? draft.start })}
                  onKeyDown={(e) => e.key === "Enter" && void submit()}
                  placeholder={timeLabel("22:00")}
                  className={`${FIELD} tabular-nums`}
                />
                <span className="text-center text-g600">to</span>
                <input
                  type="text"
                  aria-label="End time"
                  inputMode="numeric"
                  maxLength={8}
                  value={draft.end}
                  onChange={(e) => set({ end: e.target.value })}
                  onBlur={() => set({ end: normalizeTime(draft.end) ?? draft.end })}
                  onKeyDown={(e) => e.key === "Enter" && void submit()}
                  placeholder={timeLabel("08:00")}
                  className={`${FIELD} tabular-nums`}
                />
              </div>
            </Field>

            <Field label="Days" htmlFor="sched-days">
              <SelectField
                id="sched-days"
                aria-label="Days"
                value={choice}
                onChange={(e) => pickChoice(e.target.value as Choice)}
              >
                <option value="daily">Daily</option>
                <option value="weekdays">Weekdays</option>
                <option value="weekends">Weekends</option>
                <option value="choose">{chooseLabel}</option>
              </SelectField>
              {choosing && (
                <div data-testid="day-chips" className="mt-4 flex gap-1">
                  {WEEKDAYS.map((day) => (
                    <span key={day} className={day === "sat" ? "ml-3 flex-1" : "flex-1"}>
                      <button
                        type="button"
                        aria-pressed={draft.days.includes(day)}
                        data-day={day}
                        onClick={() => set({ days: toggled(draft.days, day) })}
                        className={`h-7 w-full rounded-md text-sm ${
                          draft.days.includes(day)
                            ? "border border-accent bg-primary-soft text-primary-ink"
                            : "border border-transparent bg-hover text-g600 hover:text-ink"
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
              <label className="flex w-fit cursor-pointer items-center gap-3 text-sm">
                <Switch
                  checked={draft.finishRunning}
                  label="Finish downloads in progress"
                  testId="finish-running"
                  onChange={(on) => set({ finishRunning: on })}
                />
                <span>Finish downloads in progress</span>
              </label>
            )}
            {error && <p className="text-xs text-danger-ink">{error}</p>}
          </div>

          <div className="flex shrink-0 items-center gap-2 rounded-b-lg bg-surface px-6 py-4">
            <p
              data-testid="schedule-summary"
              className={`min-w-0 flex-1 text-sm ${problem ? "text-danger-ink" : "text-g600"}`}
            >
              {problem ?? draftSummary(draft)}
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
