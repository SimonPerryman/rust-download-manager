import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { MenuCommand, MenuEntry } from "../../lib/menu";
import { Archive } from "../icons/Archive";
import { ArchiveRestore } from "../icons/ArchiveRestore";
import { ArrowDown } from "../icons/ArrowDown";
import { ArrowUp } from "../icons/ArrowUp";
import { ChevronsUp } from "../icons/ChevronsUp";
import { DownloadAgain } from "../icons/DownloadAgain";
import { ExternalLink } from "../icons/ExternalLink";
import { Folder } from "../icons/Folder";
import { Link } from "../icons/Link";
import { ListRemove } from "../icons/ListRemove";
import { Pause } from "../icons/Pause";
import { Play } from "../icons/Play";
import { Retry } from "../icons/Retry";
import { Search } from "../icons/Search";
import { Stop } from "../icons/Stop";
import { Tag } from "../icons/Tag";
import { Trash } from "../icons/Trash";

const ICON: Record<MenuCommand, (p: { size?: number }) => React.ReactNode> = {
  open: ExternalLink,
  reveal: Folder,
  locate: Search,
  forget: ListRemove,
  pause: Pause,
  resume: Play,
  startNext: ChevronsUp,
  moveUp: ArrowUp,
  moveDown: ArrowDown,
  stop: Stop,
  retry: Retry,
  retryAll: Retry,
  copyLink: Link,
  addAgain: DownloadAgain,
  deletePartial: Trash,
  tags: Tag,
  archive: Archive,
  unarchive: ArchiveRestore,
  remove: ListRemove,
};

/** Keeps the menu this far from the window's edges. */
const EDGE = 8;

type Props = {
  entries: MenuEntry[];
  /** Where it opens (the pointer, or below the focused row), in viewport pixels. */
  x: number;
  y: number;
  onRun: (cmd: MenuCommand) => void;
  onClose: () => void;
};

// The right-click menu: every action that fits the rows it was opened on.
export function RowMenu({ entries, x, y, onRun, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      left: Math.max(EDGE, Math.min(x, window.innerWidth - r.width - EDGE)),
      top: Math.max(EDGE, Math.min(y, window.innerHeight - r.height - EDGE)),
    });
  }, [x, y]);

  useEffect(() => {
    ref.current?.querySelector<HTMLButtonElement>('[role="menuitem"]')?.focus();
  }, []);

  useEffect(() => {
    const onPointer = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    document.addEventListener("pointerdown", onPointer, true);
    window.addEventListener("blur", onClose);
    window.addEventListener("resize", onClose);
    return () => {
      document.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("blur", onClose);
      window.removeEventListener("resize", onClose);
    };
  }, [onClose]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    const items = Array.from(
      ref.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? [],
    );
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    const focus = (j: number) => items[(j + items.length) % items.length]?.focus();
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        return focus(i + 1);
      case "ArrowUp":
        e.preventDefault();
        return focus(i < 0 ? -1 : i - 1);
      case "Home":
        e.preventDefault();
        return focus(0);
      case "End":
        e.preventDefault();
        return focus(-1);
      case "Escape":
      case "Tab":
        e.preventDefault();
        e.stopPropagation();
        return onClose();
    }
  };

  return (
    <div
      ref={ref}
      role="menu"
      aria-label="Download actions"
      data-testid="row-menu"
      className="row-menu"
      style={{ left: pos.left, top: pos.top }}
      onKeyDown={onKeyDown}
      onContextMenu={(e) => e.preventDefault()}
    >
      {entries.map((entry, i) => {
        if (entry.kind === "separator") return <hr key={i} role="separator" />;
        if (entry.kind === "note") {
          return (
            <p key={i} id={`row-menu-note-${i}`} role="none" className="row-menu-note">
              {entry.text}
            </p>
          );
        }
        const Icon = ICON[entry.cmd];
        // A note right after an item describes it ("The file stays on disk.").
        const note = entries[i + 1]?.kind === "note" ? `row-menu-note-${i + 1}` : undefined;
        return (
          <button
            key={entry.cmd}
            role="menuitem"
            tabIndex={-1}
            data-cmd={entry.cmd}
            aria-describedby={note}
            onClick={() => onRun(entry.cmd)}
          >
            <Icon size={16} />
            <span className="flex-1">{entry.label}</span>
            {entry.shortcut && <kbd aria-hidden="true">{entry.shortcut}</kbd>}
          </button>
        );
      })}
    </div>
  );
}
