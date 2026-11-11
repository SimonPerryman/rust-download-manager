import { formatSpeed } from "../../lib/format";
import { limitView, type LimitView } from "../../lib/rowView";
import { LIMIT_SHOWN, type LimitEditor } from "../../lib/useLimitEditor";
import type { DownloadSnapshot } from "../../lib/types";

// Live speed while downloading. A row with its own limit reads the speed against
// it ("49.8 / 50 KB/s"), or the limit alone while waiting or paused ("≤ 50 KB/s"),
// as a button that opens the editor; it's hidden while editing so a blur can't
// commit and reopen.
export function SpeedCell({ d, editor }: { d: DownloadSnapshot; editor: LimitEditor }) {
  const downloading = d.status === "downloading";
  const live = downloading && d.bytes_per_sec > 0 ? formatSpeed(d.bytes_per_sec) : "";
  if (editor.draft != null || d.limit == null || !LIMIT_SHOWN.includes(d.status)) return live;
  const v = limitView(d, d.limit);
  return (
    <button
      data-testid="row-limit"
      data-state={v.state}
      title={v.label}
      aria-label={v.label}
      onMouseDown={(e) => e.preventDefault()}
      onClick={editor.open}
      className="lim-btn"
    >
      {v.state === "capped" ? (
        <span className="text-g600">
          ≤ <Limit v={v} />
        </span>
      ) : v.state === "waiting" ? (
        <span className="lim">
          – / <Limit v={v} />
        </span>
      ) : (
        <>
          {v.speed}{" "}
          <span className="lim">
            / <Limit v={v} />
          </span>
        </>
      )}
    </button>
  );
}

// Both forms are rendered; styles/narrow.css swaps in the short one under 760px.
function Limit({ v }: { v: LimitView }) {
  return (
    <>
      <span className="lim-full">{v.limit}</span>
      <span className="lim-short">{v.short}</span>
    </>
  );
}
