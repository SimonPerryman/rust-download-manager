import { pausedNote } from "../../lib/schedule";
import { formatSpeed, statusCountsLine } from "../../lib/format";
import type { SettingsField } from "../../lib/settings";
import type { QueueState, Settings } from "../../lib/types";
import { ArrowDown } from "../icons/ArrowDown";
import { GlobalLimit } from "./GlobalLimit";

type Props = {
  /** Combined speed, bytes/s. */
  speed: number;
  downloading: number;
  waiting: number;
  /** Whether the queue window is open; a closed one is noted here once, not on each row. */
  queue: QueueState | null;
  settings: Settings | null;
  applySettings: (applied: Settings, fields: SettingsField[]) => void;
};

// The window's heartbeat along the bottom: total speed, what's running and
// waiting, and the global limit.
export function StatusBar({ speed, downloading, waiting, queue, settings, applySettings }: Props) {
  return (
    <footer
      data-testid="status-bar"
      className="col-span-full flex items-center gap-6 border-t border-line bg-sidebar px-4 text-xs text-g600"
    >
      <span
        data-testid="total-speed"
        title="Total download speed"
        className="inline-flex items-center gap-1 font-semibold tabular-nums text-ink"
      >
        <ArrowDown size={14} className="text-accent" />
        {formatSpeed(speed)}
      </span>
      <span data-testid="status-counts" className="sb-counts">
        {statusCountsLine(downloading, waiting)}
      </span>
      {pausedNote(queue) && (
        <span data-testid="queue-paused" className="font-medium text-warn-ink">
          {pausedNote(queue)}
        </span>
      )}
      <span className="flex-1" />
      <GlobalLimit settings={settings} apply={applySettings} />
    </footer>
  );
}
