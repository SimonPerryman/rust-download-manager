import { useRef } from "react";
import { formatSpeed } from "../../lib/format";
import { areaPath, linePath, niceMax, plotY, type Plot } from "../../lib/speedHistory";
import { useSize } from "../../lib/useSize";

type Props = {
  samples: readonly number[];
  /** The limit that caps this download, drawn dashed; null for none. */
  limit: number | null;
  limitLabel: string;
};

const PAD = { top: 8, bottom: 20, left: 0, right: 72 };

// A 60 s throughput graph sized to its box, with the limit as a dashed line.
export function SpeedGraph({ samples, limit, limitLabel }: Props) {
  const box = useRef<HTMLDivElement>(null);
  const { width, height } = useSize(box);
  const moving = samples.some((v) => v > 0);
  const max = niceMax(Math.max(...samples, limit ?? 0, 1024) * 1.12);
  const p: Plot = { width, height, max, ...PAD };
  const right = width - PAD.right;
  const base = height - PAD.bottom;
  return (
    <div
      ref={box}
      className="graph"
      data-testid="speed-graph"
      role="img"
      aria-label="Speed over the last 60 seconds"
    >
      {width > 40 && height > 40 && (
        <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} aria-hidden="true">
          {[0, 0.5, 1].map((k) => {
            const y = plotY(p, max * k);
            return (
              <g key={k}>
                <line x1={0} x2={right} y1={y} y2={y} stroke="var(--line-strong)" />
                <text x={width - 4} y={y + 4} textAnchor="end" className="graph-label">
                  {formatSpeed(max * k)}
                </text>
              </g>
            );
          })}
          <text x={0} y={height - 4} className="graph-label">
            60 s ago
          </text>
          <text x={right / 2} y={height - 4} textAnchor="middle" className="graph-label">
            30 s
          </text>
          <text x={right} y={height - 4} textAnchor="end" className="graph-label">
            now
          </text>
          {samples.length > 1 && (
            <>
              <path d={areaPath(samples, p)} fill="var(--primary-soft)" />
              <path
                d={linePath(samples, p)}
                fill="none"
                stroke="var(--accent)"
                strokeWidth={2}
                strokeLinejoin="round"
              />
            </>
          )}
          {limit != null ? (
            <>
              <line
                x1={0}
                x2={right}
                y1={plotY(p, limit)}
                y2={plotY(p, limit)}
                stroke="var(--warn)"
                strokeWidth={1.5}
                strokeDasharray="6 4"
                data-testid="speed-limit-line"
              />
              <text x={right - 4} y={plotY(p, limit) - 6} textAnchor="end" className="graph-limit">
                {limitLabel}
              </text>
            </>
          ) : (
            <text x={4} y={PAD.top + 12} className="graph-label">
              No limit
            </text>
          )}
          {!moving && (
            <text
              x={right / 2}
              y={(PAD.top + base) / 2}
              textAnchor="middle"
              className="graph-empty"
            >
              No transfer in the last minute
            </text>
          )}
        </svg>
      )}
    </div>
  );
}
