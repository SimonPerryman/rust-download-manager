type Props = {
  testId: "filter" | "folder-filter";
  /** `data-filter` (status key) or `data-dir` (full folder path). */
  data: { "data-filter": string } | { "data-dir": string };
  icon: React.ReactNode;
  label: string;
  /** The full folder path, for a label shortened to its last part. */
  title?: string;
  count: number;
  selected: boolean;
  /** Draw the count as a warning (Failed, when anything has). */
  alert?: boolean;
  countRef?: React.Ref<HTMLSpanElement>;
  onClick: () => void;
};

// One sidebar filter: icon, label and live count. The button is stable so the
// count element can be read and its bump animation restarted.
export function FilterButton({
  testId,
  data,
  icon,
  label,
  title,
  count,
  selected,
  alert,
  countRef,
  onClick,
}: Props) {
  return (
    <button
      data-testid={testId}
      {...data}
      data-count={count}
      aria-label={`${label}, ${count}`}
      aria-current={selected ? "true" : undefined}
      title={title ?? label}
      onClick={onClick}
      className="f-item flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-sm font-medium text-g600 hover:bg-hover hover:text-ink"
    >
      <span className="f-icon shrink-0">{icon}</span>
      <span className="f-label min-w-0 flex-1 truncate">{label}</span>
      <span ref={countRef} data-testid="filter-count" className={`f-count${alert ? " alert" : ""}`}>
        {count}
      </span>
    </button>
  );
}
