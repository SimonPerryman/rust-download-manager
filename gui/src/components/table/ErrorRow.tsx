import { describeError } from "../../lib/errors";
import type { DownloadSnapshot } from "../../lib/types";
import { AlertTriangle } from "../icons/AlertTriangle";
import type { RowHandlers } from "./rowHandlers";

// The plain-words reason under a failed row, with the ways out. The engine's
// own text stays in the tooltip.
export function ErrorRow({
  d,
  failedCount,
  handlers,
}: {
  d: DownloadSnapshot;
  /** How many downloads are failed; "Retry all failed" shows from two. */
  failedCount: number;
  handlers: RowHandlers;
}) {
  return (
    <tr data-testid="download-error" data-id={d.id} className="err-row">
      <td colSpan={8}>
        <div className="err-msg">
          <AlertTriangle size={16} className="mt-0.5 shrink-0" />
          <p className="flex-1" title={d.error ?? undefined}>
            {describeError(d.error)}
          </p>
          <button className="btn-link" onClick={() => handlers.resume(d)}>
            Retry
          </button>
          {failedCount > 1 && (
            <button className="btn-link" onClick={handlers.retryAll}>
              Retry all failed
            </button>
          )}
        </div>
      </td>
    </tr>
  );
}
