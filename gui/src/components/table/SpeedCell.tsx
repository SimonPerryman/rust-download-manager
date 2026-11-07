import { formatSpeed } from "../../lib/format";
import { LIMIT_SHOWN, type LimitEditor } from "../../lib/useLimitEditor";
import type { DownloadSnapshot } from "../../lib/types";
import { Gauge } from "../icons/Gauge";

// Live speed while downloading. A row with its own limit shows a gauge button
// instead (the limit itself until it streams, then the live speed), which opens
// the editor; it's hidden while editing so a blur can't commit and reopen.
export function SpeedCell({ d, editor }: { d: DownloadSnapshot; editor: LimitEditor }) {
  const downloading = d.status === "downloading";
  const live = downloading && d.bytes_per_sec > 0 ? formatSpeed(d.bytes_per_sec) : "";
  if (editor.draft != null || d.limit == null || !LIMIT_SHOWN.includes(d.status)) return live;
  return (
    <button
      data-testid="row-limit"
      title="Change speed limit"
      aria-label={
        downloading ? `Limited to ${formatSpeed(d.limit)}. Change speed limit` : undefined
      }
      onMouseDown={(e) => e.preventDefault()}
      onClick={editor.open}
      className="inline-flex max-w-full items-center gap-1 rounded-sm px-1 tabular-nums hover:bg-hover hover:text-ink"
    >
      <Gauge size={14} className="shrink-0 text-warn" />
      {downloading ? live : formatSpeed(d.limit)}
    </button>
  );
}
