import type { Density, Prefs } from "../../lib/prefs";
import { Rows } from "../icons/Rows";

// Compact (32 px) rows by default; pressed, rows grow to 48 px and show each file's site.
export function DensityToggle({
  density,
  onPrefs,
}: {
  density: Density;
  onPrefs: (patch: Partial<Prefs>) => void;
}) {
  const comfortable = density === "comfortable";
  return (
    <button
      aria-label="Comfortable rows"
      title="Comfortable rows"
      aria-pressed={comfortable}
      onClick={() => onPrefs({ density: comfortable ? "compact" : "comfortable" })}
      className="icon-btn"
    >
      <Rows />
    </button>
  );
}
