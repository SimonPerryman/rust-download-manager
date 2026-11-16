import type { Prefs } from "../../lib/prefs";
import { Group } from "../icons/Group";

// Pressed (the default), a batch's files sit under its group row; released, the list is flat.
export function GroupToggle({
  grouped,
  onPrefs,
}: {
  grouped: boolean;
  onPrefs: (patch: Partial<Prefs>) => void;
}) {
  return (
    <button
      aria-label="Group batches"
      title="Group batches"
      aria-pressed={grouped}
      onClick={() => onPrefs({ groupBatches: !grouped })}
      className="icon-btn"
    >
      <Group />
    </button>
  );
}
