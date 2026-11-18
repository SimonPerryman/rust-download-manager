import { useEffect, useRef, useState } from "react";
import { rescheduleChoices } from "../../lib/startAt";
import { ChevronDown } from "../icons/ChevronDown";

type Props = {
  /** Moves to a time ("YYYY-MM-DDTHH:MM"). */
  onPick: (at: string) => void;
  /** Asks for a time of the user's own. */
  onPickTime: () => void;
  /** The button's accessible name, when it needs saying whose it is. */
  label?: string;
};

// Reschedule: a menu of the real times it would land on (Tue 22:00 Tonight), then Pick a time….
export function RescheduleMenu({ onPick, onPickTime, label = "Reschedule" }: Props) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("pointerdown", onPointer, true);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer, true);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const run = (action: () => void) => () => {
    setOpen(false);
    action();
  };

  return (
    <div ref={root} className="relative">
      <button
        type="button"
        data-testid="reschedule-button"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="btn btn-line h-7"
      >
        Reschedule
        <ChevronDown size={14} />
      </button>
      {open && (
        <div
          role="menu"
          data-testid="reschedule-menu"
          className="absolute right-0 top-full z-40 mt-1 w-60 rounded-lg bg-raised p-1 text-sm shadow-dialog"
        >
          {rescheduleChoices().map((c) => (
            <button
              key={c.at}
              type="button"
              role="menuitem"
              data-testid="reschedule-choice"
              data-at={c.at}
              onClick={run(() => onPick(c.at))}
              className="grid h-8 w-full grid-cols-[100px_1fr] items-center rounded-md px-2.5 text-left hover:bg-hover"
            >
              <span className="font-medium tabular-nums">{c.when}</span>
              <span className="text-g600">{c.name}</span>
            </button>
          ))}
          <hr className="my-1 border-line" />
          <button
            type="button"
            role="menuitem"
            data-testid="reschedule-pick"
            onClick={run(onPickTime)}
            className="flex h-8 w-full items-center rounded-md px-2.5 text-left hover:bg-hover"
          >
            Pick a time…
          </button>
        </div>
      )}
    </div>
  );
}
