import { useEffect, useRef, useState } from "react";
import { scaleMarks } from "../../lib/lanes";
import { rowsHead, rowTip, rowViews, type LaneWork, type RowView } from "../../lib/laneWork";
import { precisePercent } from "../../lib/rowView";
import type { DownloadSnapshot } from "../../lib/types";

/** How long a closing lane's row takes to merge back up (matches `laneOut`). */
const LEAVE_MS = 600;

const MUTED = new Set(["paused", "cancelled", "failed", "queued"]);

/** The hovered lane and where the pointer is. */
type Tip = { lane: number; x: number; y: number };

/** A row on screen: live, or a closed lane's row still merging away. */
type Shown = RowView & { leaving?: boolean };

/**
 * `rows` plus, for a moment, the rows of lanes that just closed, so they can
 * merge back up instead of vanishing.
 */
function useLeavingRows(rows: RowView[]): Shown[] {
  const [gone, setGone] = useState<{ row: RowView; until: number }[]>([]);
  const last = useRef<RowView[]>(rows);
  const key = rows.map((r) => r.lane ?? "file").join(",");
  useEffect(() => {
    const before = last.current;
    const lanes = new Set(key.split(","));
    const left = before.filter((r) => r.lane != null && !lanes.has(String(r.lane)));
    if (left.length === 0) return;
    const until = Date.now() + LEAVE_MS;
    setGone((g) => [
      ...g.filter((x) => !lanes.has(String(x.row.lane))),
      ...left.map((row) => ({ row, until })),
    ]);
  }, [key]);
  useEffect(() => {
    last.current = rows;
  });
  useEffect(() => {
    if (gone.length === 0) return;
    const t = setTimeout(() => setGone((g) => g.filter((x) => x.until > Date.now())), LEAVE_MS);
    return () => clearTimeout(t);
  }, [gone]);
  const live = new Set(rows.map((r) => r.lane));
  const leaving = gone
    .filter((x) => !live.has(x.row.lane))
    .map((x) => ({ ...x.row, leaving: true }));
  return [...rows, ...leaving].sort((a, b) => (a.lane ?? 0) - (b.lane ?? 0));
}

const place = (b: { left: number; width: number }) => ({
  left: `${b.left}%`,
  width: `${b.width}%`,
});

// The pane's lane rows: one per live connection, each drawing only its own
// work (the piece it's on now, strong; pieces it finished, soft), so no byte
// is drawn twice. A lane that closes merges back up and its work moves to the
// top row; with no lane live, one row holds the work so far. The DOM is the
// e2e contract: `[data-testid="lane-row"][data-lane]`, under
// `[data-testid="lane-rows"]` whose `data-state` is lanes, single or done.
export function LanesMap({ d, work }: { d: DownloadSnapshot; work: LaneWork }) {
  const [tip, setTip] = useState<Tip | null>(null);
  const done = d.status === "completed";
  const rows = done ? [] : rowViews(d, work, Date.now());
  const shown = useLeavingRows(rows);
  const head = rowsHead(d, work);
  const state = done ? "done" : rows[0]?.lane != null ? "lanes" : "single";
  // Looked up each render, so the text follows new snapshots while the pointer rests.
  const tipRow = tip && state === "lanes" ? rows.find((r) => r.lane === tip.lane) : undefined;
  const text =
    tipRow?.lane != null && tipRow.piece != null ? rowTip(d, tipRow.lane, tipRow.piece) : null;
  const pct = Math.floor(precisePercent(d) ?? 0);
  const cls = ["lane-rows", done ? "done" : "", MUTED.has(d.status) ? "muted" : ""]
    .filter(Boolean)
    .join(" ");
  return (
    <div>
      <div className="map-head">
        <span data-testid="lanes-head">
          {head.lead && <span className="font-medium text-ink">{head.lead}</span>}
          {head.lead && head.rest && <span className="text-g600"> · </span>}
          {head.rest && <span className="text-g600">{head.rest}</span>}
        </span>
        {state === "lanes" && (
          <span className="text-right text-xs text-g400">Hover a lane for details.</span>
        )}
      </div>
      <div
        data-testid="lane-rows"
        data-state={state}
        className={cls}
        role="img"
        aria-label={`${[head.lead, head.rest].filter(Boolean).join(", ")}, ${pct}% downloaded`}
        onPointerLeave={() => setTip(null)}
      >
        {done ? (
          <div data-testid="lane-row" data-lane="" className="lane-row single">
            <span className="lane-label" />
            <div className="lane-track">
              <div className="lane-own" style={{ left: 0, width: "100%" }} />
            </div>
          </div>
        ) : (
          shown.map((row) => (
            <div
              key={row.lane ?? "file"}
              data-testid="lane-row"
              data-lane={row.lane ?? ""}
              className={`lane-row${row.lane == null ? " single" : ""}${row.leaving ? " leaving" : ""}`}
              onPointerMove={
                row.lane != null && !row.leaving
                  ? (e) => setTip({ lane: row.lane as number, x: e.clientX, y: e.clientY })
                  : undefined
              }
            >
              <span className="lane-label">{row.lane != null ? `Lane ${row.lane}` : ""}</span>
              <div className="lane-track">
                {row.own.map((b) => (
                  <div
                    key={b.key}
                    data-testid="lane-own"
                    className={`lane-own${b.fresh ? " fresh" : ""}`}
                    style={place(b)}
                  />
                ))}
                {row.gaps.map((b) => (
                  <div key={b.key} data-testid="lane-gap" className="lane-gap" style={place(b)} />
                ))}
                {row.now && (
                  <div data-testid="lane-now" className="lane-now" style={place(row.now)} />
                )}
              </div>
            </div>
          ))
        )}
      </div>
      <div className="lane-scale">
        {scaleMarks(d.total ?? 0).map((m, i) => (
          <span key={i}>{m}</span>
        ))}
      </div>
      {tip && text && (
        <div
          className="lane-tip"
          role="tooltip"
          style={{
            left: Math.min(tip.x + 12, window.innerWidth - 248),
            top: tip.y + 16 + 72 > window.innerHeight ? tip.y - 72 : tip.y + 16,
          }}
        >
          <strong>{text.title}</strong>
          {text.lines.map((l) => (
            <span key={l}>{l}</span>
          ))}
        </div>
      )}
    </div>
  );
}
