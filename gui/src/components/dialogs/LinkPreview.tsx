import { previewSummary, type BatchEntry } from "../../lib/batch";
import { AlertTriangle } from "../icons/AlertTriangle";
import { Check } from "../icons/Check";

const DUP_NOTE = {
  listed: "Already in your list",
  archived: "In your archive",
};

const BAD_NOTE = {
  not_a_link: "Not a link",
  unsupported_scheme: "Only http and https",
};

// What the pasted text holds, before anything starts: each link's filename
// and site, with a repeat shown once (×2) and lines that aren't links skipped.
export function LinkPreview({ entries }: { entries: BatchEntry[] }) {
  if (entries.length === 0) return null;
  const { found, skipped } = previewSummary(entries);
  return (
    <div data-testid="link-preview" className="space-y-2">
      <p data-testid="preview-summary" className="flex flex-wrap gap-x-4 text-sm">
        <span className="font-medium">{found}</span>
        {skipped && <span className="text-danger-ink">{skipped}</span>}
      </p>
      <ul className="max-h-40 overflow-y-auto rounded-md bg-surface py-1">
        {entries.map((e, i) => {
          const bad = e.kind === "bad";
          const skip = bad;
          return (
            <li
              key={i}
              data-kind={e.kind}
              className={`flex h-8 items-center gap-2 px-3 text-sm ${skip ? "text-g400" : ""}`}
            >
              {bad ? (
                <AlertTriangle size={14} className="shrink-0 text-danger" />
              ) : (
                <Check size={14} className={`shrink-0 ${skip ? "" : "text-success"}`} />
              )}
              <span className="min-w-0 flex-1 truncate" title={bad ? e.line : e.url}>
                {bad ? e.line : e.name}
              </span>
              {!bad && e.count > 1 && (
                <span
                  className="shrink-0 text-xs text-g600"
                  title={`Listed ${e.count} times, added once`}
                >
                  ×{e.count}
                </span>
              )}
              <span className={`shrink-0 text-xs ${bad ? "text-danger-ink" : "text-g400"}`}>
                {bad ? BAD_NOTE[e.reason] : e.dup ? DUP_NOTE[e.dup] : e.host}
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
