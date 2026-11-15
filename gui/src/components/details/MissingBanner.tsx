import type { DownloadSnapshot } from "../../lib/types";
import { missingBannerHeading, missingBannerText } from "../../lib/copy";
import { FileQuestion } from "../icons/FileQuestion";
import { Folder } from "../icons/Folder";
import { ListRemove } from "../icons/ListRemove";
import { Search } from "../icons/Search";
import { LocateError } from "../missing/FindPanel";

type Props = {
  d: DownloadSnapshot;
  /** Why the last Locate… was refused, if it was. */
  locateError: string | null;
  onLocate: (d: DownloadSnapshot) => void;
  onFind: (d: DownloadSnapshot) => void;
  onRemove: (d: DownloadSnapshot) => void;
};

// The details pane's note for a finished download whose file is gone: what happened and where the
// file was, then Locate… as the one primary action, Find file beside it and Remove from list quiet.
export function MissingBanner({ d, locateError, onLocate, onFind, onRemove }: Props) {
  return (
    <div data-testid="missing-banner">
      <div className="callout callout-warn">
        <FileQuestion size={20} className="mt-0.5 shrink-0" />
        <div className="min-w-0 flex-1">
          <p className="text-base font-semibold">{missingBannerHeading}</p>
          <p>{missingBannerText}</p>
          <p className="missing-path" data-testid="missing-path" title={d.final_path ?? undefined}>
            {d.final_path ?? d.target}
          </p>
        </div>
      </div>
      <div className="mb-4 flex max-w-[640px] items-center gap-2">
        <button type="button" className="btn btn-primary" onClick={() => onLocate(d)}>
          <Folder size={16} />
          Locate…
        </button>
        <button type="button" className="btn btn-soft" onClick={() => onFind(d)}>
          <Search size={16} />
          Find file
        </button>
        <span className="flex-1" />
        <button type="button" className="text-link" onClick={() => onRemove(d)}>
          <ListRemove size={14} className="mr-1" />
          Remove from list
        </button>
      </div>
      <LocateError message={locateError} />
    </div>
  );
}
