import { useState } from "react";
import { laneRows, lanesHead, laneTip, PANE_LANES, scaleMarks } from "../../lib/lanes";
import type { DownloadSnapshot } from "../../lib/types";
import { Lanes } from "../lanes/Lanes";

/** The hovered line (by its first piece) and where the pointer is. */
type Tip = { first: number; x: number; y: number };

// The pane's full-size lanes: a heading, the lanes with a tooltip per line,
// and a size scale under them. A finished download shows them merged.
export function LanesMap({ d }: { d: DownloadSnapshot }) {
  const [tip, setTip] = useState<Tip | null>(null);
  const done = d.status === "completed";
  // Looked up each render, so the text follows new snapshots while the pointer rests.
  const row =
    tip && !done ? laneRows(d.segments, PANE_LANES).find((r) => r.first === tip.first) : null;
  const text = row ? laneTip(row, d.segments.length) : null;
  return (
    <div>
      <div className="map-head">
        <span className="font-medium text-ink">{lanesHead(d)}</span>
        {!done && (
          <span className="text-right text-xs text-g400">
            Each line is one piece of the file. Hover one for details.
          </span>
        )}
      </div>
      <Lanes
        d={d}
        size="pane"
        converged={done}
        onHover={(r, e) => setTip(r ? { first: r.first, x: e.clientX, y: e.clientY } : null)}
      />
      <div className="lane-scale">
        {scaleMarks(d.total ?? 0).map((m, i) => (
          <span key={i}>{m}</span>
        ))}
      </div>
      {tip && text && (
        <div
          className="lane-tip"
          role="tooltip"
          style={{
            left: Math.min(tip.x + 12, window.innerWidth - 248),
            top: tip.y + 16 + 72 > window.innerHeight ? tip.y - 72 : tip.y + 16,
          }}
        >
          <strong>{text.title}</strong>
          {text.lines.map((l) => (
            <span key={l}>{l}</span>
          ))}
        </div>
      )}
    </div>
  );
}
