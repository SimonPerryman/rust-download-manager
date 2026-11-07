import { datesLine, summaryParts } from "../../lib/details";
import { describeError } from "../../lib/errors";
import { folderOf } from "../../lib/folders";
import { formatBytes, formatLeft, formatSpeed } from "../../lib/format";
import { hasLanes } from "../../lib/lanes";
import type { DownloadSnapshot } from "../../lib/types";
import { AlertTriangle } from "../icons/AlertTriangle";
import { Clock } from "../icons/Clock";
import { Folder } from "../icons/Folder";
import { Link } from "../icons/Link";
import { ChecksumFact } from "./ChecksumFact";
import { LanesMap } from "./LanesMap";

type Props = { d: DownloadSnapshot; onReveal: (d: DownloadSnapshot) => void };

function Stat({ big, sec }: { big: string; sec?: string }) {
  return (
    <span className="stat">
      <span className="big">{big}</span>
      {sec && <span className="ml-1 text-g600">{sec}</span>}
    </span>
  );
}

function Stats({ d }: { d: DownloadSnapshot }) {
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
          <Stat big="Waiting for a free slot" />
          <span className="text-g600">Starts when a running download finishes.</span>
        </div>
      );
    default:
      return null;
  }
}

// Overview: where the download stands, its pieces, and the facts about it.
export function OverviewTab({ d, onReveal }: Props) {
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
        {summary && (
          <div className="summary" data-testid="summary">
            <span className="big">{summary.size}</span>
            <span className="rest">{summary.rest}</span>
          </div>
        )}
        <Stats d={d} />
        {hasLanes(d) && <LanesMap d={d} />}
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
            {done && (
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
