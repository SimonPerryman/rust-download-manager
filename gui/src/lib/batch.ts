// The Add dialog's link list: what each link is, before anything starts.
// Pure presentation of the engine's reading of the pasted text (`preview_list`);
// the engine decides what counts as a link and the real filename once it connects.

import type { ParsedList, SkippedLine } from "./types";

/** One link or skipped line of the preview. */
export type BatchEntry =
  | {
      kind: "link";
      url: string;
      /** The filename the link suggests (its last path part, else its host). */
      name: string;
      host: string;
      /** How many times the paste listed it (1 when once); it is added once. */
      count: number;
      /** "listed": already in the download list, or "archived": only in the archive (both still added). */
      dup: Known | null;
    }
  | { kind: "bad"; line: string; reason: SkippedLine["reason"] };

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

/** The preview's entries from the engine's reading: links in list order, then skipped lines. */
export function entriesFrom(list: ParsedList, existing: ReadonlyMap<string, Known>): BatchEntry[] {
  const links = list.links.map((l): BatchEntry => {
    let name = l.url;
    let host = "";
    try {
      const u = new URL(l.url);
      name = suggestedName(u);
      host = u.host;
    } catch {
      // The engine vetted the link; if the webview can't read it, show it whole.
    }
    return {
      kind: "link",
      url: l.url,
      name,
      host,
      count: l.count,
      dup: existing.get(l.url) ?? null,
    };
  });
  const bad = list.skipped.map((s): BatchEntry => ({
    kind: "bad",
    line: s.line,
    reason: s.reason,
  }));
  return [...links, ...bad];
}

/** The links an Add would start, in list order (each once, however often it was listed). */
export function linksToAdd(entries: readonly BatchEntry[]): string[] {
  return entries.flatMap((e) => (e.kind === "link" ? [e.url] : []));
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
  const repeats = entries.reduce((sum, e) => sum + (e.kind === "link" ? e.count - 1 : 0), 0);
  const found = `${n} link${n === 1 ? "" : "s"} found`;
  const parts: string[] = [];
  if (bad) parts.push(bad === 1 ? "1 line isn't a link" : `${bad} lines aren't links`);
  if (repeats) parts.push(repeats === 1 ? "1 repeat" : `${repeats} repeats`);
  return { found, skipped: parts.length ? `${parts.join(" and ")}, skipped` : null };
}
