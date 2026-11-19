import { useState } from "react";
import {
  COMMAND_TITLE,
  commandLine,
  POWER_ACTIONS,
  powerLabel,
  powerHint,
  powerNote,
  POWER_TITLE,
} from "../../lib/actions";
import type { ActionsStatus, PostActions } from "../../lib/types";
import { ChevronDown } from "../icons/ChevronDown";
import { Pencil } from "../icons/Pencil";
import { CommandForm } from "./CommandForm";
import { MoreMenu } from "./MoreMenu";
import { Switch } from "./Switch";

type Props = {
  actions: PostActions;
  status: ActionsStatus | null;
  /** Saves both actions; rejects with why one was refused. */
  save: (next: PostActions) => Promise<void>;
  /** Shows a save failure that has no form to say it in. */
  onError: (e: unknown) => void;
};

// The "When downloads finish" section: two one-line rows on the rules' switch column, each a
// title and a chip that edits it. Both are global and off by default.
export function FinishSection({ actions, status, save, onError }: Props) {
  // The Edit command form is open; `enable` says a switch-on opened it, so saving also turns
  // the command on.
  const [form, setForm] = useState<{ enable: boolean } | null>(null);
  const { command, power } = actions;
  const change = (next: PostActions) => save(next).catch(onError);
  const hasCommand = command.program.trim() !== "";
  const line = commandLine(command);
  const note = status ? powerNote(status.power, power.action) : null;
  const failed = status?.power.state === "failed";

  return (
    <>
      <h2 className="mb-1.5 mt-5 text-xs font-semibold uppercase tracking-wide text-g400">
        When downloads finish
      </h2>
      <ul
        data-testid="finish-actions"
        className="rounded-lg bg-raised shadow-[inset_0_0_0_1px_var(--line)]"
      >
        <Row
          testId="finish-command"
          enabled={command.enabled}
          title={COMMAND_TITLE}
          switchLabel={`${COMMAND_TITLE} on`}
          onToggle={(on) => {
            if (on && !hasCommand) setForm({ enable: true });
            else void change({ ...actions, command: { ...command, enabled: on } });
          }}
          chip={
            <button
              type="button"
              data-testid="finish-command-chip"
              title={hasCommand ? line : undefined}
              onClick={() => setForm({ enable: false })}
              className={`${CHIP} max-w-[26rem] ${hasCommand ? "" : "text-g400"}`}
            >
              <span className="truncate">{hasCommand ? line : "No command"}</span>
              <Pencil size={13} className="shrink-0 text-g400" />
            </button>
          }
        />
        <Row
          testId="finish-power"
          enabled={power.enabled}
          title={POWER_TITLE}
          note={note}
          noteWarns={failed}
          switchLabel={`${POWER_TITLE} on`}
          onToggle={(on) => void change({ ...actions, power: { ...power, enabled: on } })}
          chip={
            <MoreMenu
              label="Power action"
              buttonTestId="finish-power-chip"
              menuTestId="finish-power-menu"
              buttonClassName={CHIP}
              trigger={
                <>
                  {powerLabel(power.action)}
                  <ChevronDown size={13} className="shrink-0 text-g400" />
                </>
              }
              items={POWER_ACTIONS.map((a) => ({
                label: powerLabel(a),
                title: powerHint(a) ?? undefined,
                checked: a === power.action,
                onSelect: () => void change({ ...actions, power: { ...power, action: a } }),
              }))}
            />
          }
        />
      </ul>
      {form && (
        <CommandForm
          initial={command}
          onClose={() => setForm(null)}
          onSubmit={async (program, args) => {
            await save({
              ...actions,
              command: { enabled: form.enable || command.enabled, program, args },
            });
            setForm(null);
          }}
        />
      )}
    </>
  );
}

const CHIP =
  "inline-flex h-7 min-w-0 items-center gap-1.5 rounded-md border border-line px-2.5 text-[13px] font-medium hover:bg-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent";

// One line in the rules' grid: title and chip (the title column is as wide as the longer title,
// so both chips start together), the switch, and a spacer where the rules have their ··· menu.
function Row({
  testId,
  enabled,
  title,
  note,
  noteWarns,
  switchLabel,
  onToggle,
  chip,
}: {
  testId: string;
  enabled: boolean;
  title: string;
  note?: string | null;
  noteWarns?: boolean;
  switchLabel: string;
  onToggle: (on: boolean) => void;
  chip: React.ReactNode;
}) {
  return (
    <li
      data-testid={testId}
      data-enabled={enabled}
      className="grid grid-cols-[1fr_auto_1.5rem] items-center gap-3 border-t border-line px-3 py-2 first:border-t-0"
    >
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-3">
          <span
            data-testid={`${testId}-title`}
            className={`w-40 shrink-0 truncate font-medium ${enabled ? "" : "text-g400"}`}
          >
            {title}
          </span>
          {chip}
        </div>
        {note && (
          <div
            data-testid={`${testId}-note`}
            className={`mt-0.5 truncate text-xs ${noteWarns ? "text-warn-ink" : enabled ? "text-g600" : "text-g400"}`}
          >
            {note}
          </div>
        )}
      </div>
      <Switch
        checked={enabled}
        label={switchLabel}
        testId={`${testId}-switch`}
        onChange={onToggle}
      />
      <span aria-hidden="true" />
    </li>
  );
}
