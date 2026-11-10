import { useRef } from "react";
import { PANE_MIN_H, statePhrase } from "../../lib/details";
import { filterKeyOf } from "../../lib/filters";
import type { LaneWork } from "../../lib/laneWork";
import type { DownloadSnapshot } from "../../lib/types";
import { X } from "../icons/X";
import { StatusGlyph } from "../table/StatusGlyph";
import { LogTab } from "./LogTab";
import { OverviewTab, type QueueSpot } from "./OverviewTab";
import { PANEL_ID, PaneTabs, tabId } from "./PaneTabs";
import { SpeedTab } from "./SpeedTab";
import { PANE_ID, type PaneTab } from "./tabs";

/** Pixels per ArrowUp / ArrowDown on the resize handle. */
const KEY_STEP = 16;

type Props = {
  d: DownloadSnapshot;
  /** What this download's lanes have written. */
  work: LaneWork;
  tab: PaneTab;
  onTab: (t: PaneTab) => void;
  height: number;
  /** The tallest the pane may grow in this window. */
  maxHeight: number;
  onResize: (px: number) => void;
  onClose: () => void;
  /** This download's last minute of speed samples. */
  samples: readonly number[];
  globalLimit: number | null;
  onReveal: (d: DownloadSnapshot) => void;
  /** Where a waiting download sits in the queue, else null. */
  spot: QueueSpot | null;
  onMove: (delta: number) => void;
};

// The selected download's details, under the table: a resize handle, the
// title and state, the tab strip and the open tab's panel.
export function DetailsPane({
  d,
  work,
  tab,
  onTab,
  height,
  maxHeight,
  onResize,
  onClose,
  samples,
  globalLimit,
  onReveal,
  spot,
  onMove,
}: Props) {
  const drag = useRef<{ y: number; h: number } | null>(null);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
    e.preventDefault();
    onResize(height + (e.key === "ArrowUp" ? KEY_STEP : -KEY_STEP));
  };

  return (
    <section
      id={PANE_ID}
      className="pane"
      data-testid="details-pane"
      data-id={d.id}
      aria-label="Download details"
      style={{ height }}
    >
      <div
        className="pane-resize"
        role="separator"
        aria-orientation="horizontal"
        aria-label="Resize details"
        aria-valuenow={height}
        aria-valuemin={PANE_MIN_H}
        aria-valuemax={maxHeight}
        tabIndex={0}
        onKeyDown={onKeyDown}
        onPointerDown={(e) => {
          drag.current = { y: e.clientY, h: height };
          e.currentTarget.setPointerCapture(e.pointerId);
        }}
        onPointerMove={(e) => {
          if (drag.current) onResize(drag.current.h + drag.current.y - e.clientY);
        }}
        onPointerUp={() => (drag.current = null)}
        onPointerCancel={() => (drag.current = null)}
      />
      <div className="pane-head">
        <div className={`pane-title st-${filterKeyOf(d.status)}`}>
          <StatusGlyph status={d.status} />
          <h2 className="truncate" title={d.filename}>
            {d.filename}
          </h2>
          <span className="pane-state">{statePhrase(d)}</span>
        </div>
        <button
          className="icon-btn"
          aria-label="Close details"
          title="Close details"
          onClick={onClose}
        >
          <X size={16} />
        </button>
      </div>
      <PaneTabs tab={tab} onTab={onTab} />
      {/* Keyed by tab so each tab opens scrolled to its own top (Log scrolls to its end). */}
      <div
        key={tab}
        className="pane-body"
        id={PANEL_ID}
        role="tabpanel"
        aria-labelledby={tabId(tab)}
      >
        {tab === "overview" && (
          <OverviewTab d={d} work={work} onReveal={onReveal} spot={spot} onMove={onMove} />
        )}
        {tab === "speed" && <SpeedTab d={d} samples={samples} globalLimit={globalLimit} />}
        {tab === "log" && <LogTab key={d.id} id={d.id} />}
      </div>
    </section>
  );
}
