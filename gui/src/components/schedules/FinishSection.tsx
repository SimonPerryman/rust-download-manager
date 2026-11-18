import { useState } from "react";
import {
  COMMAND_TITLE,
  commandLine,
  POWER_ACTIONS,
  powerLabel,
  powerNote,
  powerTitle,
} from "../../lib/actions";
import type { ActionsStatus, PostActions } from "../../lib/types";
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

// The "When downloads finish" section: two rows on the same switch column as the rules. Both
// are global and off by default.
export function FinishSection({ actions, status, save, onError }: Props) {
  // The Edit command form is open; `enable` says a switch-on opened it, so saving also turns
  // the command on.
  const [form, setForm] = useState<{ enable: boolean } | null>(null);
  const { command, power } = actions;
  const change = (next: PostActions) => save(next).catch(onError);
  const hasCommand = command.program.trim() !== "";
  const note = status ? powerNote(status.power, power.action) : null;

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
          note={hasCommand ? commandLine(command) : null}
          switchLabel={`${COMMAND_TITLE} on`}
          onToggle={(on) => {
            if (on && !hasCommand) setForm({ enable: true });
            else void change({ ...actions, command: { ...command, enabled: on } });
          }}
          menu={
            <MoreMenu
              label="Run a command options"
              buttonTestId="finish-command-menu-button"
              menuTestId="finish-command-menu"
              items={[{ label: "Edit command", onSelect: () => setForm({ enable: false }) }]}
            />
          }
        />
        <Row
          testId="finish-power"
          enabled={power.enabled}
          title={powerTitle(power.action)}
          note={note}
          switchLabel={`${powerTitle(power.action)} on`}
          onToggle={(on) => void change({ ...actions, power: { ...power, enabled: on } })}
          menu={
            <MoreMenu
              label="Power action options"
              buttonTestId="finish-power-menu-button"
              menuTestId="finish-power-menu"
              items={POWER_ACTIONS.map((a) => ({
                label: powerLabel(a),
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

function Row({
  testId,
  enabled,
  title,
  note,
  switchLabel,
  onToggle,
  menu,
}: {
  testId: string;
  enabled: boolean;
  title: string;
  note: string | null;
  switchLabel: string;
  onToggle: (on: boolean) => void;
  menu: React.ReactNode;
}) {
  return (
    <li
      data-testid={testId}
      data-enabled={enabled}
      className="grid grid-cols-[1fr_auto_auto] items-center gap-3 border-t border-line px-3 py-2 first:border-t-0"
    >
      <div className={`min-w-0 ${enabled ? "" : "text-g400"}`}>
        <div className="truncate font-medium">{title}</div>
        {note && (
          <div
            data-testid={`${testId}-note`}
            className={`truncate text-xs ${enabled ? "text-g600" : ""}`}
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
      {menu}
    </li>
  );
}
