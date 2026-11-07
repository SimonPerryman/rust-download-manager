import type { Prefs } from "../../lib/prefs";
import { Moon } from "../icons/Moon";
import { Sun } from "../icons/Sun";

// Flips to the opposite of the theme showing now (an explicit choice, even from "system").
export function ThemeToggle({
  resolved,
  onPrefs,
}: {
  resolved: "light" | "dark";
  onPrefs: (patch: Partial<Prefs>) => void;
}) {
  const next = resolved === "dark" ? "light" : "dark";
  const label = `Switch to ${next} theme`;
  return (
    <button
      aria-label={label}
      title={label}
      onClick={() => onPrefs({ theme: next })}
      className="icon-btn"
    >
      {resolved === "dark" ? <Sun /> : <Moon />}
    </button>
  );
}
