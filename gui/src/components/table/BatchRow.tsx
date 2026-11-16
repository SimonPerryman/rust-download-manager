import { failedText, rollupText, type BatchGroup } from "../../lib/batches";
import { ChevronDown } from "../icons/ChevronDown";
import { Retry } from "../icons/Retry";

// A batch's group row: its name, how it is doing, and Retry failed while something has.
// Opens and closes the batch's files. The data-* attributes are the e2e contract.
export function BatchRow({
  group,
  collapsed,
  onToggle,
  onRetry,
}: {
  group: BatchGroup;
  collapsed: boolean;
  onToggle: (id: number) => void;
  onRetry: (id: number) => void;
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
      <td className="c-prog">
        <div
          role="progressbar"
          aria-label={`${group.label} progress`}
          aria-valuenow={pct}
          aria-valuemin={0}
          aria-valuemax={100}
          className="h-1 overflow-hidden rounded-full bg-track"
        >
          <div className="h-full rounded-full bg-fill" style={{ width: `${pct}%` }} />
        </div>
      </td>
      <td colSpan={2} data-testid="batch-rollup" className="num text-g600">
        {rollupText(group)}
      </td>
      <td className="c-conn" />
      <td className="c-added" />
      <td className="c-act" />
    </tr>
  );
}
