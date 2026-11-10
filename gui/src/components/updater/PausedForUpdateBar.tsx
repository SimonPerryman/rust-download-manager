import { pausedForUpdateLine } from "../../lib/updater";
import { Check } from "../icons/Check";
import { X } from "../icons/X";

type Props = {
  /** Downloads the update restart paused that are still paused; the bar shows while > 0. */
  count: number;
  /** "Updated to rdm X" on the first run after an update; else null. */
  heading: string | null;
  onResumeAll: () => void;
  onDismiss: () => void;
};

// After an update restart: how many downloads it paused, with Resume all
// (exactly those) and a dismiss that leaves them paused.
export function PausedForUpdateBar({ count, heading, onResumeAll, onDismiss }: Props) {
  if (count === 0) return null;
  const line = pausedForUpdateLine(count);
  return (
    <div
      role="status"
      data-testid="paused-for-update"
      className="flex min-w-0 items-center gap-3 border-b border-line bg-success-soft px-4 py-2 text-sm text-ink"
    >
      <Check size={16} className="shrink-0 text-success-ink" />
      {heading ? (
        <span>
          <strong className="font-semibold">{heading}</strong> ·{" "}
          <span className="text-g600">{line}</span>
        </span>
      ) : (
        <span>{line}</span>
      )}
      <span className="ml-auto flex shrink-0 items-center gap-2">
        <button className="btn btn-line h-7" onClick={onResumeAll}>
          Resume all
        </button>
        <button
          className="icon-btn h-7 w-7"
          aria-label="Dismiss"
          title="Dismiss"
          onClick={onDismiss}
        >
          <X size={14} />
        </button>
      </span>
    </div>
  );
}
