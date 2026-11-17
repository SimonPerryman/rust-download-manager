import { useEffect, useRef, useState } from "react";
import { ruleName, ruleSummary } from "../../lib/schedule";
import type { QueueWindow } from "../../lib/types";
import { Switch } from "./Switch";

type Props = {
  rule: QueueWindow;
  onToggle: (on: boolean) => void;
  onEdit: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
};

// One rule: its name and sentence, a switch, and a ··· menu. A switched-off rule fades its
// text, not its switch, and keeps its settings.
export function RuleRow({ rule, onToggle, onEdit, onDuplicate, onDelete }: Props) {
  const [menuOpen, setMenuOpen] = useState(false);
  const menu = useRef<HTMLDivElement>(null);
  const name = ruleName(rule);

  useEffect(() => {
    if (!menuOpen) return;
    const onPointer = (e: PointerEvent) => {
      if (!menu.current?.contains(e.target as Node)) setMenuOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setMenuOpen(false);
    document.addEventListener("pointerdown", onPointer, true);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer, true);
      document.removeEventListener("keydown", onKey);
    };
  }, [menuOpen]);

  const run = (action: () => void) => () => {
    setMenuOpen(false);
    action();
  };

  return (
    <li
      data-testid="rule-row"
      data-enabled={rule.enabled}
      className="grid grid-cols-[1fr_auto_auto] items-center gap-3 border-t border-line px-3 py-2 first:border-t-0"
    >
      <div className={`min-w-0 ${rule.enabled ? "" : "text-g400"}`}>
        <div data-testid="rule-name" className="truncate font-medium">
          {name}
        </div>
        <div data-testid="rule-summary" className={`text-xs ${rule.enabled ? "text-g600" : ""}`}>
          {ruleSummary(rule)}
        </div>
      </div>
      <Switch
        checked={rule.enabled}
        label={`${name} on`}
        testId="rule-switch"
        onChange={onToggle}
      />
      <div ref={menu} className="relative">
        <button
          type="button"
          aria-label={`${name} options`}
          aria-haspopup="menu"
          aria-expanded={menuOpen}
          data-testid="rule-menu-button"
          onClick={() => setMenuOpen((o) => !o)}
          className="icon-btn icon-btn-sm tracking-widest"
        >
          ···
        </button>
        {menuOpen && (
          <div
            role="menu"
            data-testid="rule-menu"
            className="absolute right-0 top-full z-40 mt-1 w-36 rounded-lg bg-raised p-1 text-sm shadow-dialog"
          >
            {[
              ["Edit", onEdit],
              ["Duplicate", onDuplicate],
              ["Delete", onDelete],
            ].map(([label, action]) => (
              <button
                key={label as string}
                type="button"
                role="menuitem"
                onClick={run(action as () => void)}
                className="flex h-8 w-full items-center rounded-md px-2.5 text-left hover:bg-hover"
              >
                {label as string}
              </button>
            ))}
          </div>
        )}
      </div>
    </li>
  );
}
