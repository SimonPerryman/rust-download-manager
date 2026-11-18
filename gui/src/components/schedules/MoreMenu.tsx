import { useEffect, useRef, useState } from "react";
import { Check } from "../icons/Check";

export type MenuItem = {
  label: string;
  onSelect: () => void;
  /** Marks the current choice of a menu that picks one of several. */
  checked?: boolean;
};

type Props = {
  /** The button's accessible name. */
  label: string;
  items: MenuItem[];
  buttonTestId: string;
  menuTestId: string;
};

// The ··· button of a row and the menu under it; Escape or a click outside closes it.
export function MoreMenu({ label, items, buttonTestId, menuTestId }: Props) {
  const [open, setOpen] = useState(false);
  const menu = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      if (!menu.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("pointerdown", onPointer, true);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer, true);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div ref={menu} className="relative">
      <button
        type="button"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        data-testid={buttonTestId}
        onClick={() => setOpen((o) => !o)}
        className="icon-btn icon-btn-sm tracking-widest"
      >
        ···
      </button>
      {open && (
        <div
          role="menu"
          data-testid={menuTestId}
          className="absolute right-0 top-full z-40 mt-1 w-40 rounded-lg bg-raised p-1 text-sm shadow-dialog"
        >
          {items.map((item) => (
            <button
              key={item.label}
              type="button"
              role={item.checked === undefined ? "menuitem" : "menuitemradio"}
              aria-checked={item.checked}
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
              className="flex h-8 w-full items-center gap-2 rounded-md px-2.5 text-left hover:bg-hover"
            >
              {item.checked !== undefined && (
                <span className="w-3.5 shrink-0">{item.checked && <Check size={14} />}</span>
              )}
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
