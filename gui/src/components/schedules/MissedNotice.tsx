import { missedText, type StartGroup } from "../../lib/startAt";
import { AlertTriangle } from "../icons/AlertTriangle";
import { RescheduleMenu } from "./RescheduleMenu";

type Props = {
  /** The schedules whose time passed while rdm was closed; the notice shows while any remain. */
  groups: StartGroup[];
  onStartAll: () => void;
  onReschedule: (at: string) => void;
  onPickTime: () => void;
  onDismiss: () => void;
};

// Scheduled starts that were missed: leads with schedules, downloads in brackets. It is
// non-blocking and stays until dealt with, on the main list and in the Schedules view.
export function MissedNotice({ groups, onStartAll, onReschedule, onPickTime, onDismiss }: Props) {
  if (groups.length === 0) return null;
  return (
    <div
      role="status"
      data-testid="missed-notice"
      className="flex min-w-0 flex-wrap items-center gap-3 border-b border-line bg-warn-soft px-4 py-2 text-sm text-ink"
    >
      <AlertTriangle size={16} className="shrink-0 text-warn-ink" />
      <span data-testid="missed-text">{missedText(groups)}</span>
      <span className="ml-auto flex shrink-0 items-center gap-2">
        <button data-testid="missed-start-all" className="btn btn-primary h-7" onClick={onStartAll}>
          Start all
        </button>
        <RescheduleMenu onPick={onReschedule} onPickTime={onPickTime} />
        <button data-testid="missed-dismiss" className="btn btn-line h-7" onClick={onDismiss}>
          Dismiss
        </button>
      </span>
    </div>
  );
}
