import { useEffect, useRef, useState } from "react";
import { Check } from "../icons/Check";

export type MenuItem = {
  label: string;
  onSelect: () => void;
  /** Marks the current choice of a menu that picks one of several. */
  checked?: boolean;
  /** Native tooltip. */
  title?: string;
};

type Props = {
  /** The button's accessible name. */
  label: string;
  items: MenuItem[];
  buttonTestId: string;
  menuTestId: string;
  /** Replaces the ··· with this content (a chip); the button then takes `buttonClassName`. */
  trigger?: React.ReactNode;
  buttonClassName?: string;
};

// The ··· button of a row (or a chip, via `trigger`) and the menu under it. Escape or a click
// outside closes it; the arrow keys move between items.
export function MoreMenu({
  label,
  items,
  buttonTestId,
  menuTestId,
  trigger,
  buttonClassName,
}: Props) {
  const [open, setOpen] = useState(false);
  const menu = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const buttons = menu.current?.querySelectorAll<HTMLElement>("[role^=menuitem]");
    (menu.current?.querySelector<HTMLElement>("[aria-checked=true]") ?? buttons?.[0])?.focus();
    const onPointer = (e: PointerEvent) => {
      if (!menu.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        button.current?.focus();
      } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        const all = [...(buttons ?? [])];
        const at = all.indexOf(document.activeElement as HTMLElement);
        const step = e.key === "ArrowDown" ? 1 : -1;
        all[(at + step + all.length) % all.length]?.focus();
        e.preventDefault();
      }
    };
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
        ref={button}
        type="button"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        data-testid={buttonTestId}
        onClick={() => setOpen((o) => !o)}
        className={buttonClassName ?? "icon-btn icon-btn-sm tracking-widest"}
      >
        {trigger ?? "···"}
      </button>
      {open && (
        <div
          role="menu"
          data-testid={menuTestId}
          className={`absolute top-full z-40 mt-1 w-40 ${trigger ? "left-0" : "right-0"} rounded-lg bg-raised p-1 text-sm shadow-dialog`}
        >
          {items.map((item) => (
            <button
              key={item.label}
              type="button"
              role={item.checked === undefined ? "menuitem" : "menuitemradio"}
              aria-checked={item.checked}
              title={item.title}
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
