import { progressView } from "../../lib/rowView";
import type { DownloadSnapshot } from "../../lib/types";
import { AlertTriangle } from "../icons/AlertTriangle";
import { Check } from "../icons/Check";

const TONE = {
  waiting: "text-g600",
  done: "text-success-ink",
  failed: "font-medium text-danger-ink",
  stopped: "text-g600",
  plain: "tabular-nums text-ink",
};

// One 4px bar with its percentage, or a quiet word for rows that aren't moving.
export function ProgressCell({ d }: { d: DownloadSnapshot }) {
  const view = progressView(d);
  if (view.kind === "text") {
    return (
      <span className={`inline-flex items-center gap-1 text-xs ${TONE[view.tone]}`}>
        {view.tone === "done" && <Check size={14} strokeWidth={2} />}
        {view.tone === "failed" && <AlertTriangle size={14} strokeWidth={2} />}
        {view.text}
      </span>
    );
  }
  return (
    <div className="flex items-center gap-2 text-xs tabular-nums">
      <div className="h-1 min-w-0 flex-1 overflow-hidden rounded-full bg-track">
        <div
          className={`h-full rounded-full ${view.muted ? "bg-muted-fill" : "bg-fill"}`}
          style={{ width: `${view.pct}%` }}
        />
      </div>
      <span className={`min-w-10 text-right ${view.muted ? "text-g600" : "font-medium text-ink"}`}>
        {view.text}
      </span>
    </div>
  );
}
