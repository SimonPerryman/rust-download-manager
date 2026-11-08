import { useEffect, useRef } from "react";
import { STATUS_FILTERS, sameFilter, type StatusFilter, type ViewFilter } from "../../lib/filters";
import type { Folder as FolderEntry } from "../../lib/folders";
import { AlertTriangle } from "../icons/AlertTriangle";
import { ArrowDown } from "../icons/ArrowDown";
import { CheckCircle } from "../icons/CheckCircle";
import { Clock } from "../icons/Clock";
import { DownloadMark } from "../icons/DownloadMark";
import { Folder } from "../icons/Folder";
import { List } from "../icons/List";
import { Pause } from "../icons/Pause";
import { Stop } from "../icons/Stop";
import { FilterButton } from "./FilterButton";
import { Sparkline } from "./Sparkline";

const ICONS: Record<StatusFilter, React.ReactNode> = {
  all: <List />,
  downloading: <ArrowDown />,
  waiting: <Clock />,
  paused: <Pause />,
  done: <CheckCircle />,
  failed: <AlertTriangle />,
  stopped: <Stop />,
};

type Props = {
  filter: ViewFilter;
  onFilter: (f: ViewFilter) => void;
  counts: Record<StatusFilter, number>;
  folders: FolderEntry[];
  /** Bumps on each live finish; restarts the Done count's bump. 0 on load. */
  bumpSeq: number;
  /** Total speed now, and its last minute, for the sparkline. */
  speed: number;
  speedSamples: readonly number[];
  globalLimit: number | null;
};

// The status and folder filters with their live counts, and the total-speed sparkline.
export function Sidebar({
  filter,
  onFilter,
  counts,
  folders,
  bumpSeq,
  speed,
  speedSamples,
  globalLimit,
}: Props) {
  const doneCount = useRef<HTMLSpanElement>(null);

  // Remove, reflow, re-add: restarts the animation on back-to-back finishes.
  useEffect(() => {
    const el = doneCount.current;
    if (bumpSeq === 0 || !el) return;
    el.classList.remove("bump");
    void el.offsetWidth;
    el.classList.add("bump");
  }, [bumpSeq]);

  return (
    <aside className="sidebar flex min-h-0 flex-col overflow-y-auto bg-sidebar px-2 py-3">
      <div className="brand flex items-center gap-2 px-2 pb-4 pt-1 text-lg font-bold tracking-tight">
        <span className="grid h-6 w-6 place-items-center rounded-md bg-btn text-on-btn">
          <DownloadMark size={14} strokeWidth={2} />
        </span>
        <span className="brand-name">rdm</span>
      </div>
      <nav aria-label="Filter downloads">
        <ul className="grid gap-0.5">
          {STATUS_FILTERS.map((f) => {
            const view: ViewFilter = { kind: "status", status: f.key };
            return (
              <li key={f.key}>
                <FilterButton
                  testId="filter"
                  data={{ "data-filter": f.key }}
                  icon={ICONS[f.key]}
                  label={f.label}
                  count={counts[f.key]}
                  selected={sameFilter(filter, view)}
                  alert={f.key === "failed" && counts.failed > 0}
                  countRef={f.key === "done" ? doneCount : undefined}
                  onClick={() => onFilter(view)}
                />
              </li>
            );
          })}
        </ul>
        {folders.length > 0 && (
          <>
            <h2
              id="folders-head"
              className="side-head px-2 pb-2 pt-6 text-xs font-semibold text-g400"
            >
              Folders
            </h2>
            <ul className="folder-list grid gap-0.5" aria-labelledby="folders-head">
              {folders.map((f) => {
                const view: ViewFilter = { kind: "folder", dir: f.dir };
                return (
                  <li key={f.dir}>
                    <FilterButton
                      testId="folder-filter"
                      data={{ "data-dir": f.dir }}
                      icon={<Folder />}
                      label={f.label}
                      title={f.dir}
                      count={f.count}
                      selected={sameFilter(filter, view)}
                      onClick={() => onFilter(view)}
                    />
                  </li>
                );
              })}
            </ul>
          </>
        )}
      </nav>
      <Sparkline speed={speed} samples={speedSamples} globalLimit={globalLimit} />
    </aside>
  );
}
