import { useEffect, useRef, useState } from "react";
import type { Prefs, ThemePref } from "../../lib/prefs";
import type { SettingsField } from "../../lib/settings";
import type { Settings } from "../../lib/types";
import { DownloadSettings } from "./DownloadSettings";
import { UpdateSettings, type UpdateSettingsProps } from "./UpdateSettings";
import { TroubleshootingSection } from "../troubleshooting/TroubleshootingSection";
import { Monitor } from "../icons/Monitor";
import { Moon } from "../icons/Moon";
import { Sliders } from "../icons/Sliders";
import { Sun } from "../icons/Sun";

const THEME_OPTIONS: { value: ThemePref; label: string; icon: React.ReactNode }[] = [
  { value: "system", label: "System", icon: <Monitor size={14} /> },
  { value: "light", label: "Light", icon: <Sun size={14} /> },
  { value: "dark", label: "Dark", icon: <Moon size={14} /> },
];

type Props = {
  prefs: Prefs;
  onPrefs: (patch: Partial<Prefs>) => void;
  /** A `set_settings` result from the Downloads section, for the status bar to follow. */
  onApplied: (applied: Settings, fields: SettingsField[]) => void;
  /** Whether Remove (from the archive) asks first. */
  confirmRemove: boolean;
  onConfirmRemove: (ask: boolean) => void;
  updates: UpdateSettingsProps;
};

// The toolbar's settings popover: the theme (kept in localStorage), the
// engine-backed Downloads settings and Remove's confirmation, Updates, and
// Troubleshooting (last).
export function SettingsMenu({
  prefs,
  onPrefs,
  onApplied,
  confirmRemove,
  onConfirmRemove,
  updates,
}: Props) {
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
        title="Settings"
        aria-haspopup="dialog"
        aria-expanded={open}
        className={`icon-btn ${open ? "bg-primary-soft text-primary-ink" : ""}`}
      >
        <Sliders />
      </button>

      {open && (
        <div
          role="dialog"
          aria-label="Settings"
          className="absolute right-0 top-full z-30 mt-2 max-h-[calc(100vh-4.5rem)] w-80 space-y-3 overflow-y-auto rounded-lg bg-raised p-4 text-ink shadow-pop ring-1 ring-line"
        >
          <SectionHeading>Display</SectionHeading>
          <Segmented
            label="Theme"
            options={THEME_OPTIONS}
            value={prefs.theme}
            onChange={(theme) => onPrefs({ theme })}
          />
          <div className="-mx-4 h-px bg-line" />
          <SectionHeading>Downloads</SectionHeading>
          <DownloadSettings onApplied={onApplied} />
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              aria-label="Ask before removing"
              checked={confirmRemove}
              onChange={(e) => onConfirmRemove(e.target.checked)}
            />
            Ask before removing a download for good
          </label>
          <div className="-mx-4 h-px bg-line" />
          <SectionHeading>Updates</SectionHeading>
          <UpdateSettings {...updates} />
          <div className="-mx-4 h-px bg-line" />
          <TroubleshootingSection />
        </div>
      )}
    </div>
  );
}

function SectionHeading({ children }: { children: React.ReactNode }) {
  return <h3 className="text-base font-semibold tracking-tight">{children}</h3>;
}

// A labelled row of toggle buttons; the chosen one is raised.
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
      <p className="mb-1 text-xs font-medium text-g600">{label}</p>
      <div
        role="group"
        aria-label={label}
        className="flex h-8 items-center rounded-md bg-hover p-1"
      >
        {options.map((o) => (
          <button
            key={o.value}
            onClick={() => onChange(o.value)}
            aria-pressed={o.value === value}
            className={`inline-flex h-6 flex-1 items-center justify-center gap-1 rounded-sm px-2 text-sm font-medium ${
              o.value === value ? "bg-raised text-ink shadow-raised" : "text-g600 hover:text-ink"
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
