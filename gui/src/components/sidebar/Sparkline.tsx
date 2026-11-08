import { formatSpeed } from "../../lib/format";
import { areaPath, linePath, plotY, type Plot } from "../../lib/speedHistory";

type Props = {
  /** Total speed now, in bytes/s. */
  speed: number;
  samples: readonly number[];
  globalLimit: number | null;
};

// A fixed drawing box, stretched to the sidebar's width (it has no text to distort).
const W = 184;
const H = 40;
const MIN_SCALE = 10 * 1024 * 1024;

// The sidebar's total speed over the last minute, with the global limit dashed.
export function Sparkline({ speed, samples, globalLimit }: Props) {
  const max = Math.max(...samples, globalLimit ?? 0, MIN_SCALE) * 1.15;
  const p: Plot = { width: W, height: H, top: 2, bottom: 2, left: 0, right: 0, max };
  return (
    <div className="spark" data-testid="sparkline">
      <div className="text-lg font-semibold tabular-nums">{formatSpeed(speed)}</div>
      <div className="text-xs text-g400">Total speed now; the line is the last minute</div>
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden="true">
        {samples.length > 1 && (
          <>
            <path d={areaPath(samples, p)} fill="var(--primary-soft)" />
            <path
              d={linePath(samples, p)}
              fill="none"
              stroke="var(--accent)"
              strokeWidth={1.5}
              strokeLinejoin="round"
              vectorEffect="non-scaling-stroke"
            />
          </>
        )}
        {globalLimit != null && (
          <line
            x1={0}
            x2={W}
            y1={plotY(p, globalLimit)}
            y2={plotY(p, globalLimit)}
            stroke="var(--warn)"
            strokeDasharray="3 3"
            vectorEffect="non-scaling-stroke"
          />
        )}
      </svg>
    </div>
  );
}
