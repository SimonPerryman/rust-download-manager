import type { DownloadSnapshot } from "../lib/types";
import { DownloadRow } from "./DownloadRow";

type Props = {
  downloads: DownloadSnapshot[];
  empty: string;
  footer?: React.ReactNode;
};

// The bordered card wrapping a divided list of rows, or an empty-state line.
// Ported from the mockup's <section> panels.
export function DownloadList({ downloads, empty, footer }: Props) {
  if (downloads.length === 0) {
    return (
      <div className="mt-4 rounded-lg border border-border bg-card px-5 py-10 text-center text-sm text-muted-foreground shadow-sm">
        {empty}
      </div>
    );
  }
  return (
    <div className="mt-4">
      <div className="overflow-hidden rounded-lg border border-border bg-card text-card-foreground shadow-sm">
        <ul className="divide-y divide-border">
          {downloads.map((d) => (
            <DownloadRow key={d.id} d={d} />
          ))}
        </ul>
      </div>
      {footer}
    </div>
  );
}
