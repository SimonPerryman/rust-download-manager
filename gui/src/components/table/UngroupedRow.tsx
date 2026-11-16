import { UNGROUPED, ungroupedText } from "../../lib/batches";
import { ChevronDown } from "../icons/ChevronDown";

// The header over downloads in no batch: collapses them, and says how many. No bar or actions.
export function UngroupedRow({
  count,
  collapsed,
  onToggle,
}: {
  count: number;
  collapsed: boolean;
  onToggle: (id: number) => void;
}) {
  return (
    <tr data-testid="ungrouped-row" data-count={count} className="batch-row ungrouped">
      <td colSpan={8} className="c-name">
        <div className="flex min-w-0 items-center gap-2">
          <button
            type="button"
            aria-expanded={!collapsed}
            aria-label={`${collapsed ? "Show" : "Hide"} ungrouped downloads`}
            onClick={() => onToggle(UNGROUPED)}
            className="icon-btn icon-btn-sm"
          >
            <ChevronDown size={14} className={collapsed ? "-rotate-90" : ""} />
          </button>
          <span className="batch-name">Ungrouped</span>
          <span data-testid="ungrouped-count" className="text-xs text-g600">
            {ungroupedText(count)}
          </span>
        </div>
      </td>
    </tr>
  );
}
