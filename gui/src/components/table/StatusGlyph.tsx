import { STATUS_FILTERS, filterKeyOf } from "../../lib/filters";
import type { Status } from "../../lib/types";
import { AlertTriangle } from "../icons/AlertTriangle";
import { ArrowDown } from "../icons/ArrowDown";
import { CheckCircle } from "../icons/CheckCircle";
import { Clock } from "../icons/Clock";
import { Pause } from "../icons/Pause";
import { Stop } from "../icons/Stop";

const GLYPH = {
  downloading: ArrowDown,
  waiting: Clock,
  paused: Pause,
  done: CheckCircle,
  failed: AlertTriangle,
  stopped: Stop,
};

/** The UI word for an engine status ("Stopped" for cancelled, "Queued" for queued). */
export const statusLabel = (status: Status) =>
  STATUS_FILTERS.find((f) => f.key === filterKeyOf(status))!.label;

// The status icon at the start of a row, with the word for screen readers so
// colour and shape are never the only signal. The row's `st-*` class tints it.
export function StatusGlyph({ status }: { status: Status }) {
  const Icon = GLYPH[filterKeyOf(status)];
  const label = statusLabel(status);
  return (
    <>
      <span className="st-ico shrink-0" title={label}>
        <Icon />
      </span>
      <span className="sr-only">{label}: </span>
    </>
  );
}
