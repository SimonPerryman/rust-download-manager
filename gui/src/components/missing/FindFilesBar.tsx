import { findFilesBarText } from "../../lib/copy";
import type { ActiveSearch } from "../../lib/find";
import { Loader } from "../icons/Loader";

type Props = {
  /** The Find files search running; null hides the bar. */
  search: ActiveSearch | null;
  onCancel: () => void;
};

// Find files in flight: how many files, which folder of how many, and Cancel. A bar, not a modal,
// so the list stays usable while it runs.
export function FindFilesBar({ search, onCancel }: Props) {
  if (!search) return null;
  const pct = search.foldersTotal > 0 ? (search.foldersDone / search.foldersTotal) * 100 : 0;
  return (
    <div
      role="status"
      data-testid="find-files-bar"
      className="flex min-w-0 items-center gap-3 border-b border-line bg-primary-soft px-4 py-2 text-sm text-ink"
    >
      <Loader size={16} className="find-spin shrink-0" />
      <span className="min-w-0 flex-1 truncate">
        {findFilesBarText(search.ids.length, search.foldersDone, search.foldersTotal)}
      </span>
      <div className="find-bar w-40 shrink-0" aria-hidden="true">
        <i style={{ width: `${pct}%` }} />
      </div>
      <button className="btn btn-line btn-sm shrink-0" onClick={onCancel}>
        Cancel
      </button>
    </div>
  );
}
