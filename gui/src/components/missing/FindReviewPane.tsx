import { useState } from "react";
import { reviewHeading, truncatedNote } from "../../lib/copy";
import { folderOf } from "../../lib/folders";
import { initialTicks, sharedFolder, type ReviewRow } from "../../lib/find";
import type { DownloadSnapshot, FoundChoice } from "../../lib/types";
import { Check } from "../icons/Check";
import { CheckCircle } from "../icons/CheckCircle";
import { X } from "../icons/X";
import { MatchBadge } from "./MatchBadge";

type Props = {
  rows: ReviewRow[];
  height: number;
  onRelink: (choices: FoundChoice[]) => void;
  onLocate: (d: DownloadSnapshot) => void;
  onClose: () => void;
};

// What Find files (or the same-folder Search) found, one row per missing file. Matches of the same
// name and size start ticked, renamed ones unticked; nothing is relinked until Relink selected.
// Every row has Locate…, for a suggestion that is wrong or a file that wasn't found.
export function FindReviewPane({ rows, height, onRelink, onLocate, onClose }: Props) {
  const [ticked, setTicked] = useState(() => initialTicks(rows));
  const found = rows.filter((r) => r.match).length;
  const chosen = rows.flatMap((r) =>
    r.match && ticked.has(r.d.id) ? [{ id: r.d.id, path: r.match.path }] : [],
  );
  const truncated = rows.some((r) => r.result?.truncated);
  const toggle = (id: number) =>
    setTicked((t) => {
      const next = new Set(t);
      if (!next.delete(id)) next.add(id);
      return next;
    });

  return (
    <section className="pane" data-testid="find-review" aria-label="Find files" style={{ height }}>
      <div className="pane-head">
        <div className="pane-title">
          <CheckCircle size={16} className="text-success" />
          <h2 className="truncate">Find files</h2>
        </div>
        <button className="icon-btn" aria-label="Close" title="Close" onClick={onClose}>
          <X size={16} />
        </button>
      </div>
      <div className="pane-body">
        <div className="find-panel" style={{ maxWidth: 920 }}>
          <h3 data-testid="find-review-heading">
            {reviewHeading(found, rows.length, sharedFolder(rows))}
          </h3>
          <div className="find-review">
            {rows.map(({ d, match }) => (
              <div className="find-row" key={d.id} data-testid="find-review-row" data-id={d.id}>
                {match ? (
                  <button
                    type="button"
                    role="checkbox"
                    aria-checked={ticked.has(d.id)}
                    aria-label={`Relink ${d.filename}`}
                    className="find-check"
                    onClick={() => toggle(d.id)}
                  >
                    <Check size={12} strokeWidth={2.25} />
                  </button>
                ) : (
                  <span />
                )}
                <span className="min-w-0">
                  <span className="block truncate font-semibold" title={d.filename}>
                    {d.filename}
                  </span>
                  <span
                    className="block truncate font-mono text-xs text-g600"
                    title={d.final_path ?? undefined}
                  >
                    was in {folderOf(d.final_path ?? d.target)}
                  </span>
                </span>
                {match ? (
                  <span className="truncate font-mono text-xs" title={match.path}>
                    {match.path}
                  </span>
                ) : (
                  <span className="text-g600">Not found</span>
                )}
                <span>{match && <MatchBadge reason={match.reason} />}</span>
                <button className="text-link justify-self-end" onClick={() => onLocate(d)}>
                  Locate…
                </button>
              </div>
            ))}
          </div>
          {truncated && <p className="find-note mb-3 mt-0">{truncatedNote}</p>}
          <div className="flex items-center gap-2">
            <button
              className="btn btn-primary"
              disabled={chosen.length === 0}
              onClick={() => onRelink(chosen)}
            >
              Relink selected
            </button>
            <button className="text-link" onClick={onClose}>
              Close
            </button>
          </div>
        </div>
      </div>
    </section>
  );
}
