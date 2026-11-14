import { Fragment } from "react";
import type { EmptyCopy } from "../../lib/copy";
import type { Modifiers, Selection } from "../../lib/selection";
import type { SortKey, SortState } from "../../lib/sort";
import type { DownloadSnapshot } from "../../lib/types";
import { EmptyState } from "./EmptyState";
import { ErrorRow } from "./ErrorRow";
import type { RowHandlers } from "./rowHandlers";
import { SortHeader } from "./SortHeader";
import { TableRow } from "./TableRow";
import { useQueueDrag } from "./useQueueDrag";

const COLUMNS: { key: SortKey; label: string; col: string; numeric?: boolean }[] = [
  { key: "name", label: "Name", col: "c-name" },
  { key: "size", label: "Size", col: "c-size", numeric: true },
  { key: "progress", label: "Progress", col: "c-prog" },
  { key: "speed", label: "Speed", col: "c-speed", numeric: true },
  { key: "left", label: "Time left", col: "c-left", numeric: true },
  { key: "connections", label: "Connections", col: "c-conn", numeric: true },
  { key: "added", label: "Added", col: "c-added", numeric: true },
];

type Props = {
  rows: DownloadSnapshot[];
  sort: SortState;
  onSort: (key: SortKey) => void;
  unopened: ReadonlySet<number>;
  fresh: ReadonlyMap<number, number>;
  /** Failed downloads shown anywhere, for the error rows' "Retry all failed". */
  failedCount: number;
  handlers: RowHandlers;
  refocusId: number | null;
  onRefocused: () => void;
  empty: EmptyCopy;
  onAdd: () => void;
  tableRef: React.RefObject<HTMLTableElement | null>;
  selection: Selection;
  /** The waiting order, first to start first. */
  queue: readonly number[];
  /** The Queued view: every waiting row shows its drag handle. */
  queueView: boolean;
  onSelect: (id: number, mods: Modifiers) => void;
  onMenu: (id: number, x: number, y: number) => void;
  onKeyDown: (e: React.KeyboardEvent<HTMLTableElement>) => void;
  /** A waiting row dropped beside another. */
  onQueueDrop: (id: number, target: number, after: boolean) => void;
  /** The row shown in the details pane, if any. */
  detailsId: number | null;
  onDetails: (id: number, viaPointer: boolean) => void;
};

// Every download in one sortable table; a failed row is followed by its error row.
// The table takes focus for the keyboard map; rows are selected by click.
export function DownloadTable({
  rows,
  sort,
  onSort,
  unopened,
  fresh,
  failedCount,
  handlers,
  refocusId,
  onRefocused,
  empty,
  onAdd,
  tableRef,
  selection,
  queue,
  queueView,
  onSelect,
  onMenu,
  onKeyDown,
  onQueueDrop,
  detailsId,
  onDetails,
}: Props) {
  const drag = useQueueDrag(onQueueDrop);
  const dragClass = (id: number) =>
    drag.dragging === id
      ? "dragging"
      : drag.over?.id === id
        ? drag.over.after
          ? "drop-after"
          : "drop-before"
        : null;

  return (
    <div className={`list-wrap${queueView ? " queue-view" : ""}`}>
      <table
        ref={tableRef}
        className="dl"
        aria-label="Downloads"
        role="grid"
        aria-multiselectable="true"
        aria-activedescendant={selection.cursor != null ? `row-${selection.cursor}` : undefined}
        tabIndex={0}
        onKeyDown={onKeyDown}
        // Shift+F10 also raises the browser's own menu on the table; the keyboard map opens ours.
        onContextMenu={(e) => e.target === e.currentTarget && e.preventDefault()}
      >
        <thead>
          <tr>
            {COLUMNS.map((c) => (
              <SortHeader
                key={c.key}
                sortKey={c.key}
                label={c.label}
                col={c.col}
                numeric={c.numeric}
                sort={sort}
                onSort={onSort}
              />
            ))}
            <th className="c-act">
              <span className="sr-only">Actions</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((d) => (
            <Fragment key={d.id}>
              <TableRow
                d={d}
                lingering={d.status === "completed" && unopened.has(d.id)}
                freshAt={fresh.get(d.id)}
                handlers={handlers}
                refocus={refocusId === d.id}
                onRefocused={onRefocused}
                selected={selection.selected.has(d.id)}
                cursor={selection.cursor === d.id}
                queueIndex={d.status === "queued" ? queue.indexOf(d.id) : -1}
                drag={dragClass(d.id)}
                details={detailsId === d.id}
                onSelect={onSelect}
                onDetails={onDetails}
                onMenu={onMenu}
                onGripDown={drag.onGripDown}
              />
              {d.status === "failed" && (
                <ErrorRow
                  d={d}
                  failedCount={failedCount}
                  handlers={handlers}
                  selected={selection.selected.has(d.id)}
                />
              )}
            </Fragment>
          ))}
        </tbody>
      </table>
      {rows.length === 0 && <EmptyState copy={empty} onAdd={onAdd} />}
    </div>
  );
}
