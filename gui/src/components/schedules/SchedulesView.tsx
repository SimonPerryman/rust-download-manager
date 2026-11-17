import { useState } from "react";
import {
  draftOf,
  NEW_WINDOW,
  ruleOf,
  statusLine,
  withCopy,
  withoutRule,
  withRule,
  type WindowDraft,
} from "../../lib/schedule";
import type { QueueState, Schedule } from "../../lib/types";
import { RuleRow } from "./RuleRow";
import { ScheduleForm } from "./ScheduleForm";

type Props = {
  schedule: Schedule | null;
  queue: QueueState | null;
  /** Saves the whole rule set; rejects with why it was refused. */
  save: (next: Schedule) => Promise<void>;
  /** Shows a save failure that has no form to say it in. */
  onError: (e: unknown) => void;
};

// The Schedules view: one line on where the queue stands, then the rules. Each change saves
// at once; the engine applies it and says what the queue does now.
export function SchedulesView({ schedule, queue, save, onError }: Props) {
  // A form open for a new rule (null index) or for the rule at that index.
  const [form, setForm] = useState<{ index: number | null } | null>(null);
  if (!schedule) return <section className="min-h-0 flex-1 px-6 py-5" />;

  const change = (next: Schedule) => save(next).catch(onError);
  const editing = form?.index != null ? schedule.queue_windows[form.index] : undefined;

  const submit = async (draft: WindowDraft) => {
    const index = form?.index ?? undefined;
    await save(withRule(schedule, ruleOf(draft, editing?.enabled ?? true), index));
    setForm(null);
  };

  return (
    <section
      data-testid="schedules-view"
      aria-label="Schedules"
      className="min-h-0 flex-1 overflow-y-auto px-6 py-5"
    >
      <div className="mb-3 flex items-center gap-3">
        <h1 className="text-xl font-semibold tracking-tight">Schedules</h1>
        <button
          type="button"
          data-testid="new-schedule"
          onClick={() => setForm({ index: null })}
          className="btn btn-primary ml-auto"
        >
          New schedule
        </button>
      </div>
      <p data-testid="schedule-status" className="mb-4 min-h-5 text-g600">
        {statusLine(schedule, queue)}
      </p>

      <h2 className="mb-1.5 text-xs font-semibold uppercase tracking-wide text-g400">Rules</h2>
      {schedule.queue_windows.length === 0 ? (
        <p data-testid="no-rules" className="text-g600">
          No rules yet.
        </p>
      ) : (
        <ul
          data-testid="rules"
          className="rounded-lg bg-raised shadow-[inset_0_0_0_1px_var(--line)]"
        >
          {schedule.queue_windows.map((rule, i) => (
            <RuleRow
              key={i}
              rule={rule}
              onToggle={(on) => change(withRule(schedule, { ...rule, enabled: on }, i))}
              onEdit={() => setForm({ index: i })}
              onDuplicate={() => change(withCopy(schedule, i))}
              onDelete={() => change(withoutRule(schedule, i))}
            />
          ))}
        </ul>
      )}

      {form && (
        <ScheduleForm
          // A new form each time, so one rule's fields never leak into the next.
          key={form.index ?? "new"}
          initial={editing ? draftOf(editing) : NEW_WINDOW}
          editing={editing !== undefined}
          onSubmit={submit}
          onClose={() => setForm(null)}
        />
      )}
    </section>
  );
}
