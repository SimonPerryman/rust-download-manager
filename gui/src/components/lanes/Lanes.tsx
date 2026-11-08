import { CELL_LANES, laneRows, PANE_LANES, pieceBox, type LaneRow } from "../../lib/lanes";
import { precisePercent } from "../../lib/rowView";
import type { DownloadSnapshot } from "../../lib/types";

type Props = {
  d: DownloadSnapshot;
  /** Hairlines in the table's Progress cell, or the pane's full-size lanes. */
  size: "cell" | "pane";
  /** Draw the lanes merged into one finished bar (the finish moment). */
  converged?: boolean;
  /** The pane's tooltip: the lane under the pointer, or null once it leaves. */
  onHover?: (row: LaneRow | null, e: React.PointerEvent) => void;
};

const MUTED = new Set(["paused", "cancelled", "failed"]);

// One line per piece of the file, each drawn across the whole file so it shows
// where its piece sits and how far it has got. The DOM is the e2e contract:
// `[data-testid="lane"]` counts the lines.
export function Lanes({ d, size, converged = false, onHover }: Props) {
  const rows = laneRows(d.segments, size === "cell" ? CELL_LANES : PANE_LANES);
  const n = rows.length;
  const total = d.total ?? 0;
  const pct = Math.floor(precisePercent(d) ?? 0);
  const cls = [
    "lanes",
    // Spelled out in full: Tailwind drops a components-layer class it never sees literally.
    size === "cell" ? "lanes-cell" : "lanes-pane",
    n > 4 ? "dense" : "",
    MUTED.has(d.status) ? "muted" : "",
    converged ? "converged" : "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <div
      data-testid="lanes"
      data-size={size}
      className={cls}
      role="img"
      aria-label={`${n} piece${n === 1 ? "" : "s"}, ${pct}% downloaded`}
      style={{ "--n": n } as React.CSSProperties}
      onPointerLeave={onHover ? (e) => onHover(null, e) : undefined}
    >
      {rows.map((row, i) => (
        <div
          key={row.first}
          data-testid="lane"
          className="lane"
          style={
            {
              "--i": i,
              // Darker further into the file, so neighbouring lines read apart.
              "--mix": n === 1 ? "100%" : `${Math.round(25 + (75 * i) / (n - 1))}%`,
            } as React.CSSProperties
          }
          onPointerMove={onHover ? (e) => onHover(row, e) : undefined}
        >
          {row.pieces.map((s) => {
            const box = pieceBox(s, total);
            return (
              <div
                key={s.start}
                className="lane-fill"
                style={{ left: `${box.left}%`, width: `${box.fill}%` }}
              />
            );
          })}
        </div>
      ))}
    </div>
  );
}
