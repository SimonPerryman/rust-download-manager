import { useEffect, useState } from "react";
import { hasLanes, showsCellLanes, startsMerge } from "../../lib/lanes";
import { progressView } from "../../lib/rowView";
import type { DownloadSnapshot } from "../../lib/types";
import { AlertTriangle } from "../icons/AlertTriangle";
import { Check } from "../icons/Check";
import { Lanes } from "../lanes/Lanes";

/** How long a just-finished row shows its lanes merged into one bar before the quiet Done. */
export const SETTLE_MS = 1_800;

const TONE = {
  waiting: "text-g600",
  done: "text-success-ink",
  failed: "font-medium text-danger-ink",
  stopped: "text-g600",
  plain: "tabular-nums text-ink",
};

// The row's lanes (one hairline per piece of the file) with the percentage, a
// single bar when there is no tiling to draw, or a quiet word for rows that
// aren't moving. A row that finishes while watched merges its lanes into one
// bar for a moment first.
export function ProgressCell({ d }: { d: DownloadSnapshot }) {
  const view = progressView(d);
  const lanes = showsCellLanes(d);
  const [settling, setSettling] = useState(false);

  // Derived during render (not in an effect) so the finishing render keeps the
  // same lanes element, which is what lets the merge animate.
  const [prev, setPrev] = useState({ status: d.status, lanes });
  let started = false;
  if (prev.status !== d.status || prev.lanes !== lanes) {
    setPrev({ status: d.status, lanes });
    started = startsMerge(prev.lanes, d);
    setSettling(started);
  }
  const merging = (settling || started) && d.status === "completed" && hasLanes(d);
  useEffect(() => {
    if (!settling) return;
    const t = setTimeout(() => setSettling(false), SETTLE_MS);
    return () => clearTimeout(t);
  }, [settling]);

  if ((lanes && view.kind === "bar") || merging) {
    const muted = view.kind === "bar" && view.muted;
    return (
      <div className="flex h-full items-center gap-2 text-xs tabular-nums">
        <Lanes d={d} size="cell" converged={merging} />
        <span className={`min-w-10 text-right ${muted ? "text-g600" : "font-medium text-ink"}`}>
          {view.kind === "bar" ? view.text : "100%"}
        </span>
      </div>
    );
  }
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
