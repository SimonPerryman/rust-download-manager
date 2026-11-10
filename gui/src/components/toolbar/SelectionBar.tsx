import type { MenuCommand } from "../../lib/menu";
import { Archive } from "../icons/Archive";
import { ArchiveRestore } from "../icons/ArchiveRestore";
import { ExternalLink } from "../icons/ExternalLink";
import { ListRemove } from "../icons/ListRemove";
import { Pause } from "../icons/Pause";
import { Play } from "../icons/Play";
import { Retry } from "../icons/Retry";
import { Stop } from "../icons/Stop";
import { Tag } from "../icons/Tag";
import { X } from "../icons/X";

const BUTTON: Partial<
  Record<MenuCommand, { label: string; Icon: (p: { size?: number }) => React.ReactNode }>
> = {
  open: { label: "Open", Icon: ExternalLink },
  pause: { label: "Pause", Icon: Pause },
  resume: { label: "Resume", Icon: Play },
  stop: { label: "Stop", Icon: Stop },
  retry: { label: "Retry", Icon: Retry },
  tags: { label: "Tags…", Icon: Tag },
  archive: { label: "Archive", Icon: Archive },
  unarchive: { label: "Unarchive", Icon: ArchiveRestore },
  remove: { label: "Remove", Icon: ListRemove },
};

type Props = {
  count: number;
  actions: MenuCommand[];
  onRun: (cmd: MenuCommand) => void;
  onClear: () => void;
};

// The toolbar while several rows are selected: what applies to them, and a way out.
export function SelectionBar({ count, actions, onRun, onClear }: Props) {
  return (
    <div
      role="toolbar"
      aria-label="Actions for selected downloads"
      data-testid="selection-bar"
      className="flex min-w-0 flex-1 items-center gap-2"
    >
      <span
        data-testid="selection-count"
        className="mr-2 whitespace-nowrap text-base font-semibold"
      >
        {count} selected
      </span>
      <div className="flex min-w-0 flex-1 items-center gap-2 overflow-x-auto">
        {actions.map((cmd) => {
          const b = BUTTON[cmd];
          if (!b) return null;
          return (
            <button
              key={cmd}
              className="btn btn-ghost"
              aria-label={b.label}
              title={b.label}
              onClick={() => onRun(cmd)}
            >
              <b.Icon />
              <span className="sel-label">{b.label}</span>
            </button>
          );
        })}
      </div>
      <button className="btn btn-ghost" aria-label="Clear selection" onClick={onClear}>
        <X />
        <span className="sel-label">Clear selection</span>
      </button>
    </div>
  );
}
