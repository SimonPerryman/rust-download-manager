import { startsLabel, wasDueLabel, type StartGroup } from "../../lib/startAt";
import { Pencil } from "../icons/Pencil";
import { RescheduleMenu } from "./RescheduleMenu";

type Props = {
  group: StartGroup;
  onStartNow: () => void;
  /** Coming up: Edit asks for a new time. */
  onEdit: () => void;
  /** Missed: the Reschedule menu's choices. */
  onReschedule: (at: string) => void;
  onPickTime: () => void;
};

// One start schedule. Coming up has an Edit pencil (icon only, with a tooltip) and Start now (words
// only: starting is what you would regret pressing by accident); a missed one has Reschedule instead.
export function StartGroupRow({ group, onStartNow, onEdit, onReschedule, onPickTime }: Props) {
  return (
    <li
      data-testid="start-group"
      data-missed={group.missed}
      className="grid grid-cols-[1fr_auto] items-center gap-3 border-t border-line px-3 py-2 first:border-t-0"
    >
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <span data-testid="start-group-name" className="truncate font-medium">
            {group.name}
          </span>
          {group.count && (
            <span
              data-testid="start-group-count"
              className="shrink-0 rounded-full bg-hover px-2 text-xs text-g600"
            >
              {group.count}
            </span>
          )}
        </div>
        <div data-testid="start-group-when" className="text-xs text-g600">
          {group.missed ? wasDueLabel(group.at) : startsLabel(group.at)}
        </div>
      </div>
      <div className="flex items-center gap-2">
        {group.missed ? (
          <RescheduleMenu
            label={`Reschedule ${group.name}`}
            onPick={onReschedule}
            onPickTime={onPickTime}
          />
        ) : (
          <button
            type="button"
            data-testid="start-group-edit"
            aria-label={`Edit ${group.name}`}
            title="Edit"
            onClick={onEdit}
            className="icon-btn icon-btn-sm"
          >
            <Pencil size={14} />
          </button>
        )}
        <button
          type="button"
          data-testid="start-group-now"
          onClick={onStartNow}
          className={`btn h-7 ${group.missed ? "btn-primary" : "btn-line"}`}
        >
          Start now
        </button>
      </div>
    </li>
  );
}
