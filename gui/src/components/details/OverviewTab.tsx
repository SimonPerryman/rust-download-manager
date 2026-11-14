import { datesLine, summaryParts } from "../../lib/details";
import { queuePlace } from "../../lib/copy";
import { describeError } from "../../lib/errors";
import { folderOf } from "../../lib/folders";
import { formatBytes, formatLeft, formatSpeed } from "../../lib/format";
import { isMissing } from "../../lib/filters";
import { hasLanes } from "../../lib/lanes";
import type { LaneWork } from "../../lib/laneWork";
import type { DownloadSnapshot } from "../../lib/types";
import { AlertTriangle } from "../icons/AlertTriangle";
import { Clock } from "../icons/Clock";
import { Folder } from "../icons/Folder";
import { Link } from "../icons/Link";
import { Tag } from "../icons/Tag";
import { TagEditor } from "../tags/TagEditor";
import { ChecksumFact } from "./ChecksumFact";
import { MissingBanner } from "./MissingBanner";
import { LanesMap } from "./LanesMap";

/** A waiting download's place in the queue (0 starts next) and how many wait. */
export type QueueSpot = { index: number; count: number };

type Props = {
  d: DownloadSnapshot;
  /** What this download's lanes have written, for its lane rows. */
  work: LaneWork;
  onReveal: (d: DownloadSnapshot) => void;
  onLocate: (d: DownloadSnapshot) => void;
  onForget: (d: DownloadSnapshot) => void;
  spot: QueueSpot | null;
  /** Moves the download `delta` places in the queue (negative is sooner). */
  onMove: (delta: number) => void;
};

function Stat({ big, sec }: { big: string; sec?: string }) {
  return (
    <span className="stat">
      <span className="big">{big}</span>
      {sec && <span className="ml-1 text-g600">{sec}</span>}
    </span>
  );
}

function MoveButton({ label, off, onClick }: { label: string; off: boolean; onClick: () => void }) {
  return (
    <button
      className="btn btn-ghost btn-sm"
      aria-disabled={off}
      onClick={() => {
        if (!off) onClick();
      }}
    >
      {label}
    </button>
  );
}

function Stats({ d, spot, onMove }: Pick<Props, "d" | "spot" | "onMove">) {
  const of = d.total != null ? `of ${formatBytes(d.total)}` : "so far";
  switch (d.status) {
    case "downloading":
      return (
        <div className="stats">
          <Stat big={formatBytes(d.downloaded)} sec={of} />
          {d.bytes_per_sec > 0 && <Stat big={formatSpeed(d.bytes_per_sec)} />}
          {d.eta_secs != null && <Stat big={formatLeft(d.eta_secs)} sec="left" />}
        </div>
      );
    case "paused":
      return (
        <div className="stats">
          <Stat big={formatBytes(d.downloaded)} sec={of} />
          <span className="text-g600">Paused. Resume picks up where it stopped.</span>
        </div>
      );
    case "cancelled":
      return d.partial_kept ? (
        <div className="stats">
          <Stat big={formatBytes(d.downloaded)} sec={`${of} kept`} />
          <span className="text-g600">Resume picks up from here.</span>
        </div>
      ) : (
        <div className="stats">
          <Stat big="Nothing kept" />
          <span className="text-g600">Download again starts from zero.</span>
        </div>
      );
    case "queued":
      return (
        <div className="stats">
          <Stat big="Queued for a free slot" />
          <span className="text-g600">
            {spot ? queuePlace(spot.index) : "Starts when a running download finishes."}
          </span>
          {spot && spot.count > 1 && (
            // aria-disabled, not disabled: a button that disables itself under
            // the keyboard would drop focus to the page.
            <span className="queue-moves">
              <MoveButton label="Move up" off={spot.index === 0} onClick={() => onMove(-1)} />
              <MoveButton
                label="Move down"
                off={spot.index === spot.count - 1}
                onClick={() => onMove(1)}
              />
            </span>
          )}
        </div>
      );
    default:
      return null;
  }
}

// Overview: where the download stands, its lanes, and the facts about it.
export function OverviewTab({ d, work, onReveal, onLocate, onForget, spot, onMove }: Props) {
  const done = d.status === "completed";
  const summary = done ? summaryParts(d) : null;
  const dates = datesLine(d);
  return (
    <div className="ov">
      <div className="min-w-0">
        {d.status === "failed" && (
          <div className="callout">
            <AlertTriangle size={16} className="mt-0.5 shrink-0" />
            <p title={d.error ?? undefined}>{describeError(d.error)}</p>
          </div>
        )}
        {isMissing(d) && <MissingBanner d={d} onLocate={onLocate} onRemove={onForget} />}
        {summary && (
          <div className="summary" data-testid="summary">
            <span className="big">{summary.size}</span>
            <span className="rest">{summary.rest}</span>
          </div>
        )}
        <Stats d={d} spot={spot} onMove={onMove} />
        {hasLanes(d) && <LanesMap key={d.id} d={d} work={work} />}
      </div>
      <div className="facts">
        <div className="fact">
          <Link size={16} className="fact-icon" />
          <span className="url font-mono">{d.url}</span>
        </div>
        <div className="fact">
          <Folder size={16} className="fact-icon" />
          <span className="min-w-0">
            {done ? "Saved in " : "Saves to "}
            <span className="break-all text-ink">{folderOf(d.target)}</span>
            {done && !d.file_missing && (
              <>
                {" "}
                <button className="btn-link" onClick={() => onReveal(d)}>
                  Show in folder
                </button>
              </>
            )}
          </span>
        </div>
        <ChecksumFact key={d.id} d={d} />
        <div className="fact">
          <Tag size={16} className="fact-icon" />
          <div className="min-w-0 flex-1">
            <TagEditor key={d.id} rows={[d]} />
          </div>
        </div>
        {dates && (
          <div className="fact">
            <Clock size={16} className="fact-icon" />
            <span>{dates}</span>
          </div>
        )}
      </div>
    </div>
  );
}
