import { CELL_LANES, laneRows, pieceBox } from "../../lib/lanes";
import { precisePercent } from "../../lib/rowView";
import type { DownloadSnapshot } from "../../lib/types";

type Props = {
  d: DownloadSnapshot;
  /** Draw the lines merged into one finished bar (the finish moment). */
  converged?: boolean;
};

const MUTED = new Set(["paused", "cancelled", "failed"]);

// The table's Progress cell: one hairline per piece of the file, each drawn
// across the whole file so it shows where its piece sits and how far it has
// got. The DOM is the e2e contract: `[data-testid="lane"]` counts the lines.
export function Lanes({ d, converged = false }: Props) {
  const rows = laneRows(d.segments, CELL_LANES);
  const n = rows.length;
  const total = d.total ?? 0;
  const pct = Math.floor(precisePercent(d) ?? 0);
  const cls = [
    "lanes",
    "lanes-cell",
    n > 4 ? "dense" : "",
    MUTED.has(d.status) ? "muted" : "",
    converged ? "converged" : "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <div
      data-testid="lanes"
      data-size="cell"
      className={cls}
      role="img"
      aria-label={`${n} piece${n === 1 ? "" : "s"}, ${pct}% downloaded`}
      style={{ "--n": n } as React.CSSProperties}
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
