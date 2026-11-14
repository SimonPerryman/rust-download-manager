import type { DownloadSnapshot } from "../../lib/types";
import { missingBannerText } from "../../lib/copy";
import { FileQuestion } from "../icons/FileQuestion";

type Props = {
  d: DownloadSnapshot;
  onLocate: (d: DownloadSnapshot) => void;
  onRemove: (d: DownloadSnapshot) => void;
};

// The details pane's note for a finished download whose file is gone: where it was,
// Locate… as the one primary action and Remove from list as a quiet second.
export function MissingBanner({ d, onLocate, onRemove }: Props) {
  return (
    <div className="callout callout-warn" data-testid="missing-banner">
      <FileQuestion size={16} className="mt-0.5 shrink-0" />
      <div className="min-w-0 flex-1">
        <p className="font-medium">{missingBannerText}</p>
        <p className="missing-path" data-testid="missing-path" title={d.final_path ?? undefined}>
          {d.final_path ?? d.target}
        </p>
        <div className="mt-3 flex items-center gap-3">
          <button type="button" className="btn btn-primary btn-sm" onClick={() => onLocate(d)}>
            Locate…
          </button>
          <button type="button" className="text-link" onClick={() => onRemove(d)}>
            Remove from list
          </button>
        </div>
      </div>
    </div>
  );
}
