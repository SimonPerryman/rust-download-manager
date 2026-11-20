import { useState } from "react";
import {
  draftOf,
  isSpeed,
  NEW_WINDOW,
  ruleFromDraft,
  ruleName,
  rulesOf,
  statusLine,
  withCopy,
  withoutRule,
  withRule,
  type RuleDraft,
  type RuleRef,
  type ScopeChoices,
} from "../../lib/schedule";
import type { ActionsStatus, PostActions, QueueState, Schedule } from "../../lib/types";
import type { StartGroup } from "../../lib/startAt";
import { FinishSection } from "./FinishSection";
import { MissedNotice } from "./MissedNotice";
import { RuleHistoryDialog } from "./RuleHistoryDialog";
import { RuleRow } from "./RuleRow";
import { StartGroupRow } from "./StartGroupRow";
import { ScheduleForm } from "./ScheduleForm";

type Props = {
  schedule: Schedule | null;
  queue: QueueState | null;
  /** Saves the whole rule set; rejects with why it was refused. */
  save: (next: Schedule) => Promise<void>;
  /** Shows a save failure that has no form to say it in. */
  onError: (e: unknown) => void;
  /** The tags and batches a rule can be pointed at. */
  choices: ScopeChoices;
  /** Start-at schedules waiting for their time, and the ones whose time was missed. */
  upcoming: StartGroup[];
  missed: StartGroup[];
  /** The command and power action run when downloads finish, where the power one stands, and saving them. */
  finish: {
    actions: PostActions | null;
    status: ActionsStatus | null;
    save: (next: PostActions) => Promise<void>;
  };
  starts: {
    startNow: (groups: StartGroup[]) => void;
    askReschedule: (groups: StartGroup[], title?: string) => void;
    rescheduleTo: (groups: StartGroup[], at: string) => void;
    dismiss: (groups: StartGroup[]) => void;
  };
};

// The Schedules view: one line on where the queue stands, then the rules. Each change saves
// at once; the engine applies it and says what the queue does now.
export function SchedulesView({
  schedule,
  queue,
  save,
  onError,
  choices,
  upcoming,
  missed,
  finish,
  starts,
}: Props) {
  // A form open for a new rule (null) or for the rule at that place.
  const [form, setForm] = useState<{ at: RuleRef | null } | null>(null);
  // The rule whose history is open.
  const [history, setHistory] = useState<{ id: number; name: string } | null>(null);
  if (!schedule) return <section className="min-h-0 flex-1 px-6 py-5" />;

  const change = (next: Schedule) => save(next).catch(onError);
  const rules = rulesOf(schedule);
  const at = form?.at ?? null;
  const editing = at && rules.find((r) => r.kind === at.kind && r.index === at.index);

  const submit = async (draft: RuleDraft) => {
    const rule = ruleFromDraft(draft, editing?.rule.enabled ?? true, editing?.rule.id);
    await save(withRule(schedule, rule, editing ? editing.index : undefined));
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
          onClick={() => setForm({ at: null })}
          className="btn btn-primary ml-auto"
        >
          New schedule
        </button>
      </div>
      <div className="-mx-6 mb-3 empty:hidden">
        <MissedNotice
          groups={missed}
          onStartAll={() => starts.startNow(missed)}
          onReschedule={(at) => starts.rescheduleTo(missed, at)}
          onPickTime={() => starts.askReschedule(missed)}
          onDismiss={() => starts.dismiss(missed)}
        />
      </div>
      <p data-testid="schedule-status" className="mb-4 min-h-5 text-g600">
        {statusLine(schedule, queue)}
      </p>

      {missed.length > 0 && (
        <GroupList title="Missed" groups={missed} starts={starts} testId="missed-list" />
      )}
      {upcoming.length > 0 && (
        <GroupList title="Coming up" groups={upcoming} starts={starts} testId="coming-up" />
      )}

      <h2 className="mb-1.5 text-xs font-semibold uppercase tracking-wide text-g400">Rules</h2>
      {rules.length === 0 ? (
        <p data-testid="no-rules" className="text-g600">
          No rules yet.
        </p>
      ) : (
        <ul
          data-testid="rules"
          className="rounded-lg bg-raised shadow-[inset_0_0_0_1px_var(--line)]"
        >
          {rules.map((r) => (
            <RuleRow
              key={`${r.kind}${r.index}`}
              rule={r.rule}
              choices={choices}
              onToggle={(on) => change(withRule(schedule, { ...r.rule, enabled: on }, r.index))}
              onEdit={() => setForm({ at: r })}
              onHistory={
                r.rule.id && !isSpeed(r.rule)
                  ? () => setHistory({ id: r.rule.id!, name: ruleName(r.rule) })
                  : undefined
              }
              onDuplicate={() => change(withCopy(schedule, r))}
              onDelete={() => change(withoutRule(schedule, r))}
            />
          ))}
        </ul>
      )}

      {finish.actions && (
        <FinishSection
          actions={finish.actions}
          status={finish.status}
          save={finish.save}
          onError={onError}
        />
      )}

      {history && (
        <RuleHistoryDialog
          ruleId={history.id}
          name={history.name}
          onClose={() => setHistory(null)}
        />
      )}

      {form && (
        <ScheduleForm
          // A new form each time, so one rule's fields never leak into the next.
          key={at ? `${at.kind}${at.index}` : "new"}
          initial={editing ? draftOf(editing.rule) : NEW_WINDOW}
          editing={!!editing}
          choices={choices}
          onSubmit={submit}
          onClose={() => setForm(null)}
        />
      )}
    </section>
  );
}

function GroupList({
  title,
  groups,
  starts,
  testId,
}: {
  title: string;
  groups: StartGroup[];
  starts: Props["starts"];
  testId: string;
}) {
  return (
    <>
      <h2 className="mb-1.5 text-xs font-semibold uppercase tracking-wide text-g400">{title}</h2>
      <ul
        data-testid={testId}
        className="mb-5 rounded-lg bg-raised shadow-[inset_0_0_0_1px_var(--line)]"
      >
        {groups.map((g) => (
          <StartGroupRow
            key={g.group}
            group={g}
            onStartNow={() => starts.startNow([g])}
            onEdit={() => starts.askReschedule([g], "Edit start time")}
            onReschedule={(at) => starts.rescheduleTo([g], at)}
            onPickTime={() => starts.askReschedule([g])}
          />
        ))}
      </ul>
    </>
  );
}
