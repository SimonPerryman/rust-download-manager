import { DownloadMark, Pause, Plus } from "./icons";
import { ThemeToggle } from "./ThemeToggle";

type Props = {
  onAdd: () => void;
  onPauseAll: () => void;
  canPauseAll: boolean;
};

// The sticky header: mark + name, Pause all (ghost), Add download (accent),
// and the theme toggle. Ported from the mockup's <header>.
export function TopBar({ onAdd, onPauseAll, canPauseAll }: Props) {
  return (
    <header className="sticky top-0 z-20 border-b border-border bg-background/80 backdrop-blur supports-[backdrop-filter]:bg-background/60">
      <div className="flex h-14 items-center gap-3 px-5">
        <div className="flex items-center gap-2.5 select-none">
          <div className="flex h-7 w-7 items-center justify-center rounded-md bg-primary text-primary-foreground">
            <DownloadMark size={15} />
          </div>
          <span className="text-[15px] font-semibold tracking-tight">rdm</span>
        </div>

        <div className="ml-auto flex items-center gap-1.5">
          <button
            onClick={onPauseAll}
            disabled={!canPauseAll}
            className="inline-flex h-9 items-center gap-2 rounded-md px-3 text-sm font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
          >
            <Pause size={15} />
            Pause all
          </button>

          <button
            onClick={onAdd}
            className="inline-flex h-9 items-center gap-2 rounded-md bg-brand px-3.5 text-sm font-medium text-brand-foreground shadow-sm transition-colors hover:bg-brand/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
          >
            <Plus size={15} />
            Add download
          </button>

          <div className="mx-1 h-5 w-px bg-border" />

          <ThemeToggle />
        </div>
      </div>
    </header>
  );
}
