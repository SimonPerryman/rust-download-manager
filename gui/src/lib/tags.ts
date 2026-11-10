// Tags as the user types them, checked against the engine's rules (see
// src/engine/tags.rs) so a form can say what's wrong before anything is sent.
// The engine checks again and has the last word.

import type { DownloadSnapshot } from "./types";

/** The engine's `MAX_TAGS`: most tags one download may carry. */
export const MAX_TAGS = 16;
/** The engine's `MAX_TAG_LEN`: longest tag, in characters. */
export const MAX_TAG_LEN = 32;

/** The comparison key for a tag: case doesn't make a different tag. */
export const tagKey = (tag: string) => tag.trim().toLowerCase();

/** Whether `d` carries `tag`, ignoring case. */
export function hasTag(d: DownloadSnapshot, tag: string): boolean {
  const key = tagKey(tag);
  return d.tags.some((t) => tagKey(t) === key);
}

/** Comma-separated text as tags: trimmed, blanks dropped, repeats (ignoring case) dropped. */
export function parseTags(text: string): string[] {
  const out: string[] = [];
  for (const piece of text.split(",")) {
    const tag = piece.trim();
    if (tag && !out.some((t) => tagKey(t) === tagKey(tag))) out.push(tag);
  }
  return out;
}

/** What's wrong with adding `tags` to a download that has `existing`, or null if nothing. */
export function tagsError(
  tags: readonly string[],
  existing: readonly string[] = [],
): string | null {
  const long = tags.find((t) => [...t].length > MAX_TAG_LEN);
  if (long) return `“${long}” is too long. A tag is at most ${MAX_TAG_LEN} characters.`;
  const keys = new Set(existing.map(tagKey));
  for (const t of tags) keys.add(tagKey(t));
  if (keys.size > MAX_TAGS) return `A download can have at most ${MAX_TAGS} tags.`;
  return null;
}

/** One sidebar tag: its label (the first spelling seen) and how many downloads carry it. */
export interface TagCount {
  tag: string;
  count: number;
}

/** Every tag on `ds`, once each ignoring case, with its count; sorted by label. */
export function tagCounts(ds: readonly DownloadSnapshot[]): TagCount[] {
  const byKey = new Map<string, TagCount>();
  for (const d of ds) {
    for (const t of d.tags) {
      const entry = byKey.get(tagKey(t));
      if (entry) entry.count += 1;
      else byKey.set(tagKey(t), { tag: t, count: 1 });
    }
  }
  return [...byKey.values()].sort(
    (a, b) =>
      a.tag.localeCompare(b.tag, undefined, { sensitivity: "base" }) || (a.tag < b.tag ? -1 : 1),
  );
}

/** The tags across `ds`, once each ignoring case, in first-seen order: what a tag editor lists. */
export function unionTags(ds: readonly DownloadSnapshot[]): string[] {
  const out: string[] = [];
  for (const d of ds) {
    for (const t of d.tags) if (!out.some((o) => tagKey(o) === tagKey(t))) out.push(t);
  }
  return out;
}
