import type { SortKey, SortState } from "../../lib/sort";
import { ChevronDown } from "../icons/ChevronDown";

type Props = {
  sortKey: SortKey;
  label: string;
  /** The column's width class (also hides it in narrow windows). */
  col: string;
  numeric?: boolean;
  sort: SortState;
  onSort: (key: SortKey) => void;
};

// A sortable column header; the active one carries `aria-sort` and its chevron
// points up for ascending.
export function SortHeader({ sortKey, label, col, numeric, sort, onSort }: Props) {
  const active = sort.key === sortKey;
  return (
    <th
      className={`${col}${numeric ? " num" : ""}`}
      aria-sort={active ? (sort.dir === "asc" ? "ascending" : "descending") : undefined}
    >
      <button className="th-btn" data-sort={sortKey} onClick={() => onSort(sortKey)}>
        {label}
        <ChevronDown size={14} className={active && sort.dir === "asc" ? "rotate-180" : ""} />
      </button>
    </th>
  );
}
