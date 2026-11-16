import { useState } from "react";
import { previewSummary, refusalMessage, type BatchEntry, type LinkEntry } from "../../lib/batch";
import { AlertTriangle } from "../icons/AlertTriangle";
import { Check } from "../icons/Check";
import { ChevronDown } from "../icons/ChevronDown";

const DUP_NOTE = {
  listed: "Already in your list",
  archived: "In your archive",
};

const BAD_NOTE = {
  not_a_link: "Not a link",
  unsupported_scheme: "Only http and https",
};

/** How many of a range's links show before "N more". */
const PEEK = 3;

// The ranges the engine refused, under the links box (a refusal blocks nothing else).
export function RangeErrors({ entries }: { entries: BatchEntry[] }) {
  const refused = entries.filter(
    (e): e is Extract<BatchEntry, { kind: "refused" }> => e.kind === "refused",
  );
  if (refused.length === 0) return null;
  return (
    <div data-testid="range-errors" className="space-y-1 text-xs text-danger-ink">
      {refused.map((e, i) => (
        <p key={i} title={e.line}>
          {refused.length > 1 && <span className="font-mono">{e.line} </span>}
          {refusalMessage(e)}
        </p>
      ))}
    </div>
  );
}

// A range's row: its file span and link count, opening to the links it made.
function PatternRow({ label, links }: { label: string; links: LinkEntry[] }) {
  const [open, setOpen] = useState(false);
  const shown = open ? links.slice(0, PEEK) : [];
  return (
    <>
      <li
        data-kind="pattern"
        className="mx-1 flex h-7 items-center gap-2 rounded-sm bg-hover px-2 text-sm font-medium"
      >
        <button
          type="button"
          aria-expanded={open}
          aria-label={`${open ? "Hide" : "Show"} ${label}`}
          onClick={() => setOpen((o) => !o)}
          className="-ml-1 grid size-5 shrink-0 place-items-center rounded-sm text-g600 hover:text-ink"
        >
          <ChevronDown size={14} className={open ? "" : "-rotate-90"} />
        </button>
        <span className="min-w-0 flex-1 truncate" title={label}>
          {label}
        </span>
        <span className="shrink-0 text-xs font-normal text-g600">
          {links.length} link{links.length === 1 ? "" : "s"}
        </span>
      </li>
      {shown.map((l) => (
        <li
          key={l.url}
          data-kind="pattern-link"
          className="flex h-7 items-center gap-2 pl-[34px] pr-3 text-sm"
        >
          <span className="min-w-0 flex-1 truncate" title={l.url}>
            {l.name}
          </span>
          {l.count > 1 && (
            <span
              className="shrink-0 text-xs text-g600"
              title={`Listed ${l.count} times, added once`}
            >
              ×{l.count}
            </span>
          )}
          {l.dup && <span className="shrink-0 text-xs text-g400">{DUP_NOTE[l.dup]}</span>}
        </li>
      ))}
      {open && links.length > PEEK && (
        <li
          data-kind="pattern-more"
          className="flex h-7 items-center pl-[34px] pr-3 text-sm text-g400"
        >
          {links.length - PEEK} more
        </li>
      )}
    </>
  );
}

// What the pasted text holds, before anything starts: each link's filename
// and site, with a repeat shown once (×2) and lines that aren't links skipped.
export function LinkPreview({ entries }: { entries: BatchEntry[] }) {
  const rows = entries.filter((e) => e.kind !== "refused");
  if (rows.length === 0) return null;
  const { found, skipped } = previewSummary(entries);
  return (
    <div data-testid="link-preview" className="space-y-2">
      <p data-testid="preview-summary" className="flex flex-wrap gap-x-4 text-sm">
        <span className="font-medium">{found}</span>
        {skipped && <span className="text-danger-ink">{skipped}</span>}
      </p>
      <ul className="max-h-40 overflow-y-auto rounded-md bg-surface py-1">
        {rows.map((e, i) => {
          if (e.kind === "pattern") {
            return <PatternRow key={`${i}-${e.links[0].url}`} label={e.label} links={e.links} />;
          }
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
