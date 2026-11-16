import { failedText, rollupText, type BatchGroup } from "../../lib/batches";
import { ChevronDown } from "../icons/ChevronDown";
import { Folder } from "../icons/Folder";
import { Retry } from "../icons/Retry";

// A batch's group row: its name, how it is doing, and Retry failed while something has.
// Opens and closes the batch's files. The data-* attributes are the e2e contract.
export function BatchRow({
  group,
  collapsed,
  onToggle,
  onRetry,
  onReveal,
}: {
  group: BatchGroup;
  collapsed: boolean;
  onToggle: (id: number) => void;
  onRetry: (id: number) => void;
  /** Open the folder the batch's files went into. */
  onReveal: (id: number) => void;
}) {
  const pct = Math.round(group.fraction * 100);
  return (
    <tr
      data-testid="batch-row"
      data-batch={group.id}
      data-done={group.done}
      data-failed={group.failed}
      data-count={group.items.length}
      className="batch-row"
    >
      {/* The Size cell has nothing to say here, so the name takes its room. */}
      <td colSpan={2} className="c-name">
        <div className="flex min-w-0 items-center gap-2">
          <button
            type="button"
            aria-expanded={!collapsed}
            aria-label={`${collapsed ? "Show" : "Hide"} ${group.label}`}
            onClick={() => onToggle(group.id)}
            className="icon-btn icon-btn-sm"
          >
            <ChevronDown size={14} className={collapsed ? "-rotate-90" : ""} />
          </button>
          <span data-testid="batch-name" className="batch-name truncate" title={group.label}>
            {group.label}
          </span>
          {group.failed > 0 && (
            <>
              <span data-testid="batch-failed" className="batch-pill">
                {failedText(group)}
              </span>
              <button
                type="button"
                data-testid="batch-retry"
                className="btn btn-soft btn-sm"
                onClick={() => onRetry(group.id)}
              >
                <Retry size={14} />
                Retry failed
              </button>
            </>
          )}
        </div>
      </td>
      {/* The bar and its count, as a file's bar and its percentage; the count has the room of Speed and Time left. */}
      <td colSpan={3} className="c-prog">
        <div className="flex items-center gap-3 text-xs tabular-nums">
          <div
            role="progressbar"
            aria-label={`${group.label} progress`}
            aria-valuenow={pct}
            aria-valuemin={0}
            aria-valuemax={100}
            className="h-1 w-24 shrink-0 overflow-hidden rounded-full bg-track"
          >
            <div className="h-full rounded-full bg-fill" style={{ width: `${pct}%` }} />
          </div>
          <span data-testid="batch-rollup" className="font-medium text-ink">
            {rollupText(group)}
          </span>
        </div>
      </td>
      <td className="c-conn" />
      <td className="c-added" />
      <td className="c-act">
        {/* Right-aligned with the per-file Show in folder, which sits left of Archive. */}
        <div className="flex items-center justify-end gap-1">
          <button
            type="button"
            data-testid="batch-reveal"
            aria-label="Show in folder"
            title="Show in folder"
            onClick={() => onReveal(group.id)}
            className="icon-btn icon-btn-sm"
          >
            <Folder size={14} />
          </button>
          <span className="w-6" aria-hidden="true" />
        </div>
      </td>
    </tr>
  );
}
