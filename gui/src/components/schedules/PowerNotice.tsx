import { countdownText } from "../../lib/actions";
import type { ActionsStatus } from "../../lib/types";
import { AlertTriangle } from "../icons/AlertTriangle";

type Props = {
  status: ActionsStatus | null;
  onCancel: () => void;
};

// The power action is counting down: it says what will happen and when, and Cancel stops it.
// Shown on every page, since the countdown must be hard to miss.
export function PowerNotice({ status, onCancel }: Props) {
  if (status?.power.state !== "countdown") return null;
  const { action, seconds_left } = status.power;
  return (
    <div
      role="alert"
      data-testid="power-notice"
      className="flex min-w-0 flex-wrap items-center gap-3 border-b border-line bg-warn-soft px-4 py-2 text-sm text-ink"
    >
      <AlertTriangle size={16} className="shrink-0 text-warn-ink" />
      <span data-testid="power-countdown">{countdownText(action, seconds_left)}</span>
      <span className="ml-auto flex shrink-0 items-center gap-2">
        <button data-testid="power-cancel" className="btn btn-primary h-7" onClick={onCancel}>
          Cancel
        </button>
      </span>
    </div>
  );
}
