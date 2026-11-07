import { effectiveLimit } from "../../lib/details";
import { formatSpeed } from "../../lib/format";
import { speedStats } from "../../lib/speedHistory";
import type { DownloadSnapshot } from "../../lib/types";
import { SpeedGraph } from "./SpeedGraph";

type Props = {
  d: DownloadSnapshot;
  samples: readonly number[];
  globalLimit: number | null;
};

// Speed: now, average and peak over the last minute, and the graph.
export function SpeedTab({ d, samples, globalLimit }: Props) {
  const stats = speedStats(samples);
  const limit = effectiveLimit(d.limit, globalLimit);
  const own = limit != null && limit === d.limit;
  const label = limit == null ? "" : `${own ? "Limit" : "Global limit"} ${formatSpeed(limit)}`;
  return (
    <div className="spd">
      <div className="spd-stats">
        <span>
          <b>{d.status === "downloading" ? formatSpeed(d.bytes_per_sec) : "Idle"}</b>{" "}
          <span className="text-g600">now</span>
        </span>
        <span>
          <b>{stats ? formatSpeed(stats.average) : "None"}</b>{" "}
          <span className="text-g600">average</span>
        </span>
        <span>
          <b>{stats ? formatSpeed(stats.peak) : "None"}</b>{" "}
          <span className="text-g600">peak in the last minute</span>
        </span>
      </div>
      <SpeedGraph samples={samples} limit={limit} limitLabel={label} />
    </div>
  );
}
