// The Add dialog's link list: what each link is, before anything starts.
// Pure presentation of the engine's reading of the pasted text (`preview_list`);
// the engine decides what counts as a link and the real filename once it connects.

import type { ParsedList, SkippedLine } from "./types";

/** One link, a range's links, or a skipped line of the preview. */
export type BatchEntry =
  | LinkEntry
  | {
      kind: "pattern";
      /** "lesson01.mp4 to lesson12.mp4": the range's first and last file. */
      label: string;
      /** The links the range made, in order. */
      links: LinkEntry[];
    }
  | { kind: "bad"; line: string; reason: "not_a_link" | "unsupported_scheme" }
  | { kind: "refused"; line: string; reason: RangeProblem; count: number | null };

/** The skip reasons that mean a `[a-b]` range was refused. */
export type RangeProblem = Exclude<SkippedLine["reason"], "not_a_link" | "unsupported_scheme">;

/** A link of the preview. */
export type LinkEntry = {
  kind: "link";
  url: string;
  /** The filename the link suggests (its last path part, else its host). */
  name: string;
  host: string;
  /** How many times the paste listed it (1 when once); it is added once. */
  count: number;
  /** "listed": already in the download list, or "archived": only in the archive (both still added). */
  dup: Known | null;
};

/** Where a URL already is: in the working list, or only in the archive. */
export type Known = "listed" | "archived";

/** Each known URL's place; a URL both listed and archived counts as listed. */
export function knownUrls(ds: readonly { url: string; archived: boolean }[]): Map<string, Known> {
  const out = new Map<string, Known>();
  for (const d of ds) {
    if (!d.archived) out.set(d.url, "listed");
    else if (!out.has(d.url)) out.set(d.url, "archived");
  }
  return out;
}

/** The name a link suggests: its last path segment, decoded, or its host. */
function suggestedName(u: URL): string {
  const last = u.pathname.split("/").filter(Boolean).pop() ?? "";
  try {
    return decodeURIComponent(last) || u.host;
  } catch {
    return last || u.host;
  }
}

const RANGE_PROBLEMS = new Set<SkippedLine["reason"]>([
  "range_in_host",
  "range_invalid",
  "range_width_mismatch",
  "range_reversed",
  "multiple_ranges",
  "range_too_big",
]);

/** The most links one range may make (the engine's `MAX_PATTERN_LINKS`). */
const MAX_RANGE_LINKS = 500;

/** What's wrong with a refused range, as a sentence. */
export function refusalMessage(e: Extract<BatchEntry, { kind: "refused" }>): string {
  switch (e.reason) {
    case "range_too_big":
      return e.count == null
        ? `That range makes more than ${MAX_RANGE_LINKS} links.`
        : `That range makes ${e.count.toLocaleString("en-US")} links. The most is ${MAX_RANGE_LINKS}.`;
    case "range_in_host":
      return "A range can't be in the host.";
    case "range_invalid":
      return "A range's ends must both be numbers or both be single letters.";
    case "range_width_mismatch":
      return "A range's ends must have the same number of digits.";
    case "range_reversed":
      return "A range must count up, like [1-9].";
    case "multiple_ranges":
      return "Only one range per link.";
  }
}

function linkEntry(
  l: ParsedList["links"][number],
  existing: ReadonlyMap<string, Known>,
): LinkEntry {
  let name = l.url;
  let host = "";
  try {
    const u = new URL(l.url);
    name = suggestedName(u);
    host = u.host;
  } catch {
    // The engine vetted the link; if the webview can't read it, show it whole.
  }
  return { kind: "link", url: l.url, name, host, count: l.count, dup: existing.get(l.url) ?? null };
}

/**
 * The preview's entries from the engine's reading: links in list order with a
 * range's links gathered into one row, then skipped lines and refused ranges.
 */
export function entriesFrom(list: ParsedList, existing: ReadonlyMap<string, Known>): BatchEntry[] {
  const out: BatchEntry[] = [];
  const rows = new Map<number, Extract<BatchEntry, { kind: "pattern" }>>();
  for (const l of list.links) {
    const link = linkEntry(l, existing);
    if (l.pattern == null) {
      out.push(link);
      continue;
    }
    let row = rows.get(l.pattern);
    if (!row) {
      row = { kind: "pattern", label: "", links: [] };
      rows.set(l.pattern, row);
      out.push(row);
    }
    row.links.push(link);
  }
  for (const row of rows.values()) {
    const [first, last] = [row.links[0], row.links[row.links.length - 1]];
    row.label = first === last ? first.name : `${first.name} to ${last.name}`;
  }
  for (const s of list.skipped) {
    out.push(
      RANGE_PROBLEMS.has(s.reason)
        ? { kind: "refused", line: s.line, reason: s.reason as RangeProblem, count: s.count }
        : { kind: "bad", line: s.line, reason: s.reason as "not_a_link" | "unsupported_scheme" },
    );
  }
  return out;
}

/** Every link of the preview, a range's included, in list order. */
function allLinks(entries: readonly BatchEntry[]): LinkEntry[] {
  return entries.flatMap((e) => (e.kind === "link" ? [e] : e.kind === "pattern" ? e.links : []));
}

/** The links an Add would start, in list order (each once, however often it was listed). */
export function linksToAdd(entries: readonly BatchEntry[]): string[] {
  return allLinks(entries).map((l) => l.url);
}

/** How a batch add went: how many went in, and the links that didn't with their errors. */
export interface AddOutcome {
  added: number;
  failed: { url: string; error: string }[];
}

/** Adds `links` one at a time, in order, carrying on past a link the engine rejects. */
export async function addLinks(
  links: readonly string[],
  add: (url: string) => Promise<unknown>,
): Promise<AddOutcome> {
  const failed: AddOutcome["failed"] = [];
  for (const url of links) {
    try {
      await add(url);
    } catch (e) {
      failed.push({ url, error: String(e) });
    }
  }
  return { added: links.length - failed.length, failed };
}

/** The Add dialog's error after some links went in and some didn't. */
export function partialAddError(outcome: AddOutcome): string {
  const { added, failed } = outcome;
  const which = failed.length === 1 ? "This link" : "These links";
  return `Added ${added}. ${which} couldn't be added: ${failed[0].error}`;
}

/** The Add dialog's submit label for `n` links. */
export function addLabel(n: number): string {
  return n >= 2 ? `Add ${n} downloads` : "Add download";
}

/** The line above the preview: how many links, and what will be skipped. */
export function previewSummary(entries: readonly BatchEntry[]): {
  found: string;
  skipped: string | null;
} {
  const n = linksToAdd(entries).length;
  const bad = entries.filter((e) => e.kind === "bad").length;
  const repeats = allLinks(entries).reduce((sum, l) => sum + l.count - 1, 0);
  const found = `${n} link${n === 1 ? "" : "s"} found`;
  const parts: string[] = [];
  if (bad) parts.push(bad === 1 ? "1 line isn't a link" : `${bad} lines aren't links`);
  if (repeats) parts.push(repeats === 1 ? "1 repeat" : `${repeats} repeats`);
  return { found, skipped: parts.length ? `${parts.join(" and ")}, skipped` : null };
}
