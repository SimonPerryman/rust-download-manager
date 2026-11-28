import { useState } from "react";
import { previewSummary, refusalMessage, type BatchEntry, type LinkEntry } from "../../lib/batch";
import { Check } from "../icons/Check";
import { ChevronDown } from "../icons/ChevronDown";
import { Info } from "../icons/Info";

const DUP_NOTE = {
  listed: "Already in your list",
  archived: "In your archive",
};

/** How many of a range's links show before "N more". */
const PEEK = 3;

/** What a link already on disk can be asked: add it anyway (or take that back), or find its file. */
export type ExistingActions = {
  onToggle: (url: string) => void;
  onReveal: (url: string) => void;
};

// The tail of a link already on disk: the chip, and the row's two actions. Such a link
// starts un-ticked; Download anyway ticks it, Skip takes it back.
function OnDisk({ link, actions }: { link: LinkEntry; actions?: ExistingActions }) {
  return (
    <>
      <span
        data-testid="on-disk-chip"
        className="shrink-0 rounded-sm bg-hover px-1.5 py-0.5 text-xs font-medium text-g600"
      >
        Already downloaded
      </span>
      {actions && (
        <>
          <button
            type="button"
            data-testid="on-disk-toggle"
            className="btn-link shrink-0 text-xs"
            onClick={() => actions.onToggle(link.url)}
          >
            {link.ticked ? "Skip" : "Download anyway"}
          </button>
          <button
            type="button"
            className="btn-link shrink-0 text-xs"
            onClick={() => actions.onReveal(link.url)}
          >
            Show in folder
          </button>
        </>
      )}
    </>
  );
}

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

// A range's row: its file span and link count, opening to the links it made (the first few,
// then "N more" for the rest).
function PatternRow({
  label,
  links,
  open,
  all,
  onToggle,
  onShowAll,
  actions,
}: {
  label: string;
  links: LinkEntry[];
  open: boolean;
  all: boolean;
  onToggle: () => void;
  onShowAll: () => void;
  actions?: ExistingActions;
}) {
  const shown = open ? (all ? links : links.slice(0, PEEK)) : [];
  return (
    <>
      <li
        data-kind="pattern"
        className="mx-1 flex h-8 items-center gap-2 rounded-sm bg-hover px-2 text-sm font-medium"
      >
        <button
          type="button"
          aria-expanded={open}
          aria-label={`${open ? "Hide" : "Show"} ${label}`}
          onClick={onToggle}
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
          className="flex h-8 items-center gap-2 pl-[34px] pr-3 text-sm"
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
          {l.onDisk ? (
            <OnDisk link={l} actions={actions} />
          ) : (
            l.dup && <span className="shrink-0 text-xs text-g400">{DUP_NOTE[l.dup]}</span>
          )}
        </li>
      ))}
      {open && links.length > PEEK && (
        <li data-kind="pattern-more" className="flex h-8 items-center pl-[34px] pr-3 text-sm">
          <button type="button" className="btn-link" onClick={onShowAll}>
            {all ? "Show less" : `${links.length - PEEK} more`}
          </button>
        </li>
      )}
    </>
  );
}

// What the pasted text holds, before anything starts: each link's filename
// and site, with a repeat shown once (×2) and lines that aren't links skipped.
// A link already in the list or archive gets a neutral info mark, not a tick; it is still added.
// A link whose file is already in the folder gets a chip and starts un-ticked (see OnDisk).
export function LinkPreview({
  entries,
  actions,
}: {
  entries: BatchEntry[];
  actions?: ExistingActions;
}) {
  // Which ranges are open, and which of those list every link; keyed by the range's place.
  const [open, setOpen] = useState<ReadonlySet<number>>(new Set());
  const [all, setAll] = useState<ReadonlySet<number>>(new Set());
  const [reasonsOpen, setReasonsOpen] = useState(false);
  const seen = entries.filter((e) => e.kind !== "refused");
  if (seen.length === 0) return null;
  // Lines that aren't links are listed only on request, under the skipped count.
  const rows = seen.filter(
    (e): e is LinkEntry | Extract<BatchEntry, { kind: "pattern" }> =>
      e.kind === "link" || e.kind === "pattern",
  );
  const { found, skipped } = previewSummary(entries);
  const flip = (set: ReadonlySet<number>, i: number) => {
    const next = new Set(set);
    if (!next.delete(i)) next.add(i);
    return next;
  };
  return (
    <div data-testid="link-preview" className="space-y-2">
      <p data-testid="preview-summary" className="flex flex-wrap items-baseline gap-x-4 text-sm">
        <span className="font-medium">{found}</span>
        {skipped && (
          <button
            type="button"
            data-testid="skipped-toggle"
            aria-expanded={reasonsOpen}
            onClick={() => setReasonsOpen((o) => !o)}
            className="inline-flex items-center gap-0.5 text-g600 hover:text-ink"
          >
            <ChevronDown size={12} className={reasonsOpen ? "" : "-rotate-90"} />
            {skipped.label}
          </button>
        )}
      </p>
      {skipped && reasonsOpen && (
        <ul data-testid="skipped-reasons" className="space-y-0.5 pl-4 text-xs text-g600">
          {skipped.reasons.map((r) => (
            <li key={r}>{r}</li>
          ))}
        </ul>
      )}
      {/* An open range makes room for its links; the list scrolls past that. */}
      <ul
        className={`overflow-y-auto rounded-md bg-surface py-1 ${open.size > 0 ? "max-h-80" : "max-h-40"}`}
      >
        {rows.map((e, i) => {
          if (e.kind === "pattern") {
            return (
              <PatternRow
                key={`${i}-${e.links[0].url}`}
                label={e.label}
                links={e.links}
                open={open.has(i)}
                all={all.has(i)}
                onToggle={() => {
                  setOpen((o) => flip(o, i));
                  setAll((a) => {
                    const next = new Set(a);
                    next.delete(i);
                    return next;
                  });
                }}
                onShowAll={() => setAll((a) => flip(a, i))}
                actions={actions}
              />
            );
          }
          return (
            <li
              key={i}
              data-kind={e.kind}
              data-dup={e.dup ?? undefined}
              data-ticked={e.ticked}
              className="flex h-8 items-center gap-2 px-3 text-sm"
            >
              {e.dup || !e.ticked ? (
                <Info size={14} className="shrink-0 text-g400" />
              ) : (
                <Check size={14} className="shrink-0 text-success" />
              )}
              <span className="min-w-0 flex-1 truncate" title={e.url}>
                {e.name}
              </span>
              {e.count > 1 && (
                <span
                  className="shrink-0 text-xs text-g600"
                  title={`Listed ${e.count} times, added once`}
                >
                  ×{e.count}
                </span>
              )}
              {e.onDisk ? (
                <OnDisk link={e} actions={actions} />
              ) : (
                <span className="shrink-0 text-xs text-g400">
                  {e.dup ? DUP_NOTE[e.dup] : e.host}
                </span>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
