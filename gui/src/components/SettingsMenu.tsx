import { useEffect, useRef, useState } from "react";
import type { Density, ThemePref } from "../lib/prefs";
import { usePrefs } from "../lib/usePrefs";
import { DownloadSettings } from "./DownloadSettings";
import { Monitor } from "./icons/Monitor";
import { Moon } from "./icons/Moon";
import { Sliders } from "./icons/Sliders";
import { Sun } from "./icons/Sun";

const THEME_OPTIONS: { value: ThemePref; label: string; icon: React.ReactNode }[] = [
  { value: "system", label: "System", icon: <Monitor size={14} /> },
  { value: "light", label: "Light", icon: <Sun size={14} /> },
  { value: "dark", label: "Dark", icon: <Moon size={14} /> },
];

const DENSITY_OPTIONS: { value: Density; label: string }[] = [
  { value: "comfortable", label: "Comfortable" },
  { value: "compact", label: "Compact" },
];

// Header settings popover: cosmetic display prefs (theme + row density, kept
// in localStorage) and the engine-backed Downloads settings.
export function SettingsMenu() {
  const [prefs, update] = usePrefs();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        aria-label="Settings"
        aria-haspopup="dialog"
        aria-expanded={open}
        className={`inline-flex h-9 w-9 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
          open ? "bg-accent text-foreground" : ""
        }`}
      >
        <Sliders size={17} />
      </button>

      {open && (
        <div
          role="dialog"
          aria-label="Settings"
          className="absolute right-0 top-full z-30 mt-2 w-80 space-y-3 rounded-lg border border-border bg-popover p-3 text-popover-foreground shadow-lg"
        >
          <SectionHeading>Display</SectionHeading>
          <Segmented
            label="Theme"
            options={THEME_OPTIONS}
            value={prefs.theme}
            onChange={(theme) => update({ theme })}
          />
          <Segmented
            label="Density"
            options={DENSITY_OPTIONS}
            value={prefs.density}
            onChange={(density) => update({ density })}
          />
          <div className="-mx-3 h-px bg-border" />
          <SectionHeading>Downloads</SectionHeading>
          <DownloadSettings />
        </div>
      )}
    </div>
  );
}

function SectionHeading({ children }: { children: React.ReactNode }) {
  return <h3 className="px-0.5 text-[13px] font-semibold tracking-tight">{children}</h3>;
}

// A labelled row of toggle buttons in the Tabs control's visual style.
function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: { value: T; label: string; icon?: React.ReactNode }[];
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <div>
      <p className="mb-1.5 px-0.5 text-xs font-medium text-muted-foreground">{label}</p>
      <div
        role="group"
        aria-label={label}
        className="flex h-9 items-center rounded-lg bg-muted p-1 text-muted-foreground"
      >
        {options.map((o) => (
          <button
            key={o.value}
            onClick={() => onChange(o.value)}
            aria-pressed={o.value === value}
            className={`inline-flex h-7 flex-1 items-center justify-center gap-1.5 rounded-md px-2 text-[13px] font-medium transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
              o.value === value
                ? "bg-background text-foreground shadow-sm"
                : "hover:text-foreground"
            }`}
          >
            {o.icon}
            {o.label}
          </button>
        ))}
      </div>
    </div>
  );
}
