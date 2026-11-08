import { Fragment } from "react";
import type { EmptyCopy } from "../../lib/copy";
import type { SortKey, SortState } from "../../lib/sort";
import type { DownloadSnapshot } from "../../lib/types";
import { EmptyState } from "./EmptyState";
import { ErrorRow } from "./ErrorRow";
import type { RowHandlers } from "./rowHandlers";
import { SortHeader } from "./SortHeader";
import { TableRow } from "./TableRow";

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
  wrapRef: React.RefObject<HTMLDivElement>;
  selectedId: number | null;
  onSelect: (id: number | null) => void;
};

// Every download in one sortable table; a failed row is followed by its error row.
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
  wrapRef,
  selectedId,
  onSelect,
}: Props) {
  return (
    <div ref={wrapRef} className="list-wrap" tabIndex={-1}>
      <table className="dl" aria-label="Downloads">
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
                selected={selectedId === d.id}
                onSelect={onSelect}
              />
              {d.status === "failed" && (
                <ErrorRow d={d} failedCount={failedCount} handlers={handlers} />
              )}
            </Fragment>
          ))}
        </tbody>
      </table>
      {rows.length === 0 && <EmptyState copy={empty} onAdd={onAdd} />}
    </div>
  );
}
