// The Add dialog's link list: what each pasted line is, before anything starts.
// Pure presentation; the engine decides the real filename once it connects.

import { isHttpUrl } from "./url";

/** One pasted line (or one link from a line holding several). */
export type BatchEntry =
  | {
      kind: "link";
      url: string;
      /** The filename the link suggests (its last path part, else its host). */
      name: string;
      host: string;
      /**
       * "listed": already in the download list, or "archived": only in the archive (both
       * still added); "repeat": earlier in this paste (skipped).
       */
      dup: Known | "repeat" | null;
    }
  | { kind: "bad"; line: string };

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

/**
 * Splits pasted text into entries: one link per line (a line of several
 * whitespace-separated links counts as several), blank and `#` comment lines
 * dropped, anything else flagged as not a link.
 */
export function parseLinks(text: string, existing: ReadonlyMap<string, Known>): BatchEntry[] {
  const seen = new Set<string>();
  const entries: BatchEntry[] = [];
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const parts = line.split(/\s+/);
    const pieces = parts.length > 1 && parts.every(isHttpUrl) ? parts : [line];
    for (const piece of pieces) {
      if (!isHttpUrl(piece)) {
        entries.push({ kind: "bad", line: piece });
        continue;
      }
      const u = new URL(piece);
      const dup = seen.has(piece) ? "repeat" : (existing.get(piece) ?? null);
      seen.add(piece);
      entries.push({ kind: "link", url: piece, name: suggestedName(u), host: u.host, dup });
    }
  }
  return entries;
}

/** The links an Add would start, in paste order (repeats inside the paste left out). */
export function linksToAdd(entries: readonly BatchEntry[]): string[] {
  return entries.flatMap((e) => (e.kind === "link" && e.dup !== "repeat" ? [e.url] : []));
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
  const repeats = entries.filter((e) => e.kind === "link" && e.dup === "repeat").length;
  const found = `${n} link${n === 1 ? "" : "s"} found`;
  const parts: string[] = [];
  if (bad) parts.push(bad === 1 ? "1 line isn't a link" : `${bad} lines aren't links`);
  if (repeats) parts.push(repeats === 1 ? "1 repeat" : `${repeats} repeats`);
  return { found, skipped: parts.length ? `${parts.join(" and ")}, skipped` : null };
}
