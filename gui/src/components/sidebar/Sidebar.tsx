import { useEffect, useRef } from "react";
import {
  isArchivedView,
  STATUS_FILTERS,
  sameFilter,
  type StatusFilter,
  type ViewFilter,
} from "../../lib/filters";
import type { BatchGroup } from "../../lib/batches";
import type { Folder as FolderEntry } from "../../lib/folders";
import type { TagCount } from "../../lib/tags";
import { Archive } from "../icons/Archive";
import { AlertTriangle } from "../icons/AlertTriangle";
import { ArrowDown } from "../icons/ArrowDown";
import { CheckCircle } from "../icons/CheckCircle";
import { Clock } from "../icons/Clock";
import { DownloadMark } from "../icons/DownloadMark";
import { FileQuestion } from "../icons/FileQuestion";
import { Folder } from "../icons/Folder";
import { Group } from "../icons/Group";
import { List } from "../icons/List";
import { Pause } from "../icons/Pause";
import { Stop } from "../icons/Stop";
import { Tag } from "../icons/Tag";
import { FilterButton } from "./FilterButton";
import { Sparkline } from "./Sparkline";

const ICONS: Record<StatusFilter, React.ReactNode> = {
  all: <List />,
  downloading: <ArrowDown />,
  waiting: <Clock />,
  paused: <Pause />,
  done: <CheckCircle />,
  failed: <AlertTriangle />,
  missing: <FileQuestion />,
  stopped: <Stop />,
};

type Props = {
  filter: ViewFilter;
  onFilter: (f: ViewFilter) => void;
  counts: Record<StatusFilter, number>;
  folders: FolderEntry[];
  archivedCount: number;
  /** The Batches section: each batch's working files. */
  batches: BatchGroup[];
  /** The Tags section: the archived rows' tags while an Archived view is shown, else the rest's. */
  tags: TagCount[];
  /** Bumps on each live finish; restarts the Done count's bump. 0 on load. */
  bumpSeq: number;
  /** Total speed now, and its last minute, for the sparkline. */
  speed: number;
  speedSamples: readonly number[];
  globalLimit: number | null;
  /** The Schedules view is showing, so no filter is the current one. */
  schedulesOpen: boolean;
  onSchedules: () => void;
};

// The status and folder filters with their live counts, and the total-speed sparkline.
export function Sidebar({
  filter,
  onFilter,
  counts,
  folders,
  archivedCount,
  batches,
  tags,
  bumpSeq,
  speed,
  speedSamples,
  globalLimit,
  schedulesOpen,
  onSchedules,
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

  const inArchive = isArchivedView(filter);
  const archivedView: ViewFilter = { kind: "archived" };

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
                  selected={!schedulesOpen && sameFilter(filter, view)}
                  alert={f.key === "failed" && counts.failed > 0}
                  warn={f.key === "missing" && counts.missing > 0}
                  countRef={f.key === "done" ? doneCount : undefined}
                  onClick={() => onFilter(view)}
                />
              </li>
            );
          })}
        </ul>
        <ul className="mt-3 grid gap-0.5">
          <li>
            <FilterButton
              testId="filter"
              data={{ "data-filter": "archived" }}
              icon={<Archive />}
              label="Archived"
              count={archivedCount}
              selected={!schedulesOpen && inArchive}
              onClick={() => onFilter(archivedView)}
            />
          </li>
        </ul>
        <h2
          id="automation-head"
          className="side-head px-2 pb-2 pt-6 text-xs font-semibold text-g400"
        >
          Automation
        </h2>
        <ul className="grid gap-0.5" aria-labelledby="automation-head">
          <li>
            <button
              data-testid="schedules-link"
              aria-current={schedulesOpen ? "true" : undefined}
              onClick={onSchedules}
              className="f-item flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-sm font-medium text-g600 hover:bg-hover hover:text-ink"
            >
              <span className="f-icon shrink-0">
                <Clock />
              </span>
              <span className="f-label min-w-0 flex-1 truncate">Schedules</span>
            </button>
          </li>
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
                      selected={!schedulesOpen && sameFilter(filter, view)}
                      onClick={() => onFilter(view)}
                    />
                  </li>
                );
              })}
            </ul>
          </>
        )}
        {batches.length > 0 && !inArchive && (
          <>
            <h2
              id="batches-head"
              className="side-head px-2 pb-2 pt-6 text-xs font-semibold text-g400"
            >
              Batches
            </h2>
            <ul className="batch-list grid gap-0.5" aria-labelledby="batches-head">
              {batches.map((b) => {
                const view: ViewFilter = { kind: "batch", batch: b.id };
                return (
                  <li key={b.id}>
                    <FilterButton
                      testId="batch-filter"
                      data={{ "data-batch": String(b.id) }}
                      icon={<Group />}
                      label={b.label}
                      count={b.items.length}
                      selected={!schedulesOpen && sameFilter(filter, view)}
                      onClick={() => onFilter(view)}
                    />
                  </li>
                );
              })}
            </ul>
          </>
        )}
        {tags.length > 0 && (
          <>
            <h2 id="tags-head" className="side-head px-2 pb-2 pt-6 text-xs font-semibold text-g400">
              {inArchive ? "Archived tags" : "Tags"}
            </h2>
            <ul className="tag-list grid gap-0.5" aria-labelledby="tags-head">
              {tags.map((t) => {
                const view: ViewFilter = { kind: "tag", tag: t.tag, archived: inArchive };
                return (
                  <li key={t.tag}>
                    <FilterButton
                      testId="tag-filter"
                      data={{ "data-tag": t.tag }}
                      icon={<Tag />}
                      label={t.tag}
                      count={t.count}
                      selected={!schedulesOpen && sameFilter(filter, view)}
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
