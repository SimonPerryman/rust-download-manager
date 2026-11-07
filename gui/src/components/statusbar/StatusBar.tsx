import { formatSpeed, statusCountsLine } from "../../lib/format";
import type { SettingsField } from "../../lib/settings";
import type { Settings } from "../../lib/types";
import { ArrowDown } from "../icons/ArrowDown";
import { GlobalLimit } from "./GlobalLimit";

type Props = {
  /** Combined speed, bytes/s. */
  speed: number;
  downloading: number;
  waiting: number;
  settings: Settings | null;
  applySettings: (applied: Settings, fields: SettingsField[]) => void;
};

// The window's heartbeat along the bottom: total speed, what's running and
// waiting, and the global limit.
export function StatusBar({ speed, downloading, waiting, settings, applySettings }: Props) {
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
      <span className="flex-1" />
      <GlobalLimit settings={settings} apply={applySettings} />
    </footer>
  );
}
