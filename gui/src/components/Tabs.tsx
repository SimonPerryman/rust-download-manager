export type Tab = "active" | "history";

type Props = {
  tab: Tab;
  onChange: (t: Tab) => void;
  activeCount: number;
  historyCount: number;
};

// The segmented Active/History control with live counts. Ported from the
// mockup; the selected tab gets the raised `bg-background` treatment.
export function Tabs({ tab, onChange, activeCount, historyCount }: Props) {
  return (
    <div className="inline-flex h-9 items-center justify-center rounded-lg bg-muted p-1 text-muted-foreground">
      <TabButton
        label="Active"
        count={activeCount}
        selected={tab === "active"}
        onClick={() => onChange("active")}
      />
      <TabButton
        label="History"
        count={historyCount}
        selected={tab === "history"}
        onClick={() => onChange("history")}
      />
    </div>
  );
}

function TabButton({
  label,
  count,
  selected,
  onClick,
}: {
  label: string;
  count: number;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-3 text-sm font-medium transition-all focus-visible:outline-none ${
        selected ? "bg-background text-foreground shadow-sm" : ""
      }`}
    >
      {label}
      <span className="ml-0.5 inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-foreground/10 px-1 text-[11px] font-semibold tabular-nums">
        {count}
      </span>
    </button>
  );
}
