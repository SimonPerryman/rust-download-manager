// Target folders for the sidebar's folder filters, derived from each
// download's target path. Pure presentation: grouping and labels only.

import type { DownloadSnapshot } from "./types";

const SEPARATOR = /[\\/]/;
const DRIVE = /^[A-Za-z]:$/;

/** The parent dir of `target`; `/` and `\` both separate, and a root keeps its separator. */
export function folderOf(target: string): string {
  const idx = Math.max(target.lastIndexOf("/"), target.lastIndexOf("\\"));
  if (idx < 0) return "";
  const dir = target.slice(0, idx);
  return dir === "" || DRIVE.test(dir) ? target.slice(0, idx + 1) : dir;
}

export interface Folder {
  dir: string;
  label: string;
  count: number;
}

const isRoot = (dir: string) => dir === "" || SEPARATOR.test(dir[dir.length - 1]);

/** `dir`'s last `k` components, with its own separators (the whole dir when shorter). */
function suffix(dir: string, k: number): string {
  let start = dir.length;
  for (let i = 0; i < k; i++) {
    const prev = Math.max(dir.lastIndexOf("/", start - 1), dir.lastIndexOf("\\", start - 1));
    if (prev < 0) return dir;
    start = prev;
  }
  return dir.slice(start + 1);
}

const depth = (dir: string) => dir.split(SEPARATOR).filter(Boolean).length;

/** Per dir, the shortest trailing part no other dir in the list shares. */
function labels(dirs: string[]): Map<string, string> {
  const out = new Map<string, string>();
  for (const dir of dirs) {
    if (isRoot(dir)) {
      out.set(dir, dir);
      continue;
    }
    let k = 1;
    while (
      k < depth(dir) &&
      dirs.some((o) => o !== dir && !isRoot(o) && suffix(o, k) === suffix(dir, k))
    ) {
      k += 1;
    }
    out.set(dir, suffix(dir, k));
  }
  return out;
}

/**
 * One folder per distinct target dir in `all`, with its count of `shown` rows
 * (so a folder fully hidden by a pending clear stays listed with 0). Sorted by
 * label, then dir.
 */
export function deriveFolders(all: DownloadSnapshot[], shown: DownloadSnapshot[]): Folder[] {
  const dirs = [...new Set(all.map((d) => folderOf(d.target)))];
  const counts = new Map<string, number>();
  for (const d of shown) {
    const dir = folderOf(d.target);
    counts.set(dir, (counts.get(dir) ?? 0) + 1);
  }
  const label = labels(dirs);
  return dirs
    .map((dir) => ({ dir, label: label.get(dir)!, count: counts.get(dir) ?? 0 }))
    .sort((a, b) => a.label.localeCompare(b.label) || (a.dir < b.dir ? -1 : a.dir > b.dir ? 1 : 0));
}
