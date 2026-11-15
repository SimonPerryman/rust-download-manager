// Find file / Find files, as the window sees it: the search in flight, the kept result per missing
// download, and the review of a many-file search. The engine decides every match and keeps its own
// copy of the results; this only follows its events so the panels can show them.

import { folderOf } from "./folders";
import { isMissing } from "./filters";
import type { DownloadSnapshot, FindEvent, FindResult, FoundFile } from "./types";

/** A search the window started and has not seen end. */
export interface ActiveSearch {
  /** The engine's id; null until the start call answers (events may arrive first). */
  search: number | null;
  /** The missing downloads it looks for. */
  ids: number[];
  /** Find files (a review at the end) rather than Find file (the record's own panel). */
  many: boolean;
  foldersDone: number;
  foldersTotal: number;
  current: string | null;
}

export interface FindState {
  /** The latest result per missing download, until it is skipped, relinked or its file changes. */
  results: Record<number, FindResult>;
  active: ActiveSearch | null;
  /** The downloads a finished many-file search is being reviewed for; null when closed. */
  review: number[] | null;
}

export const initialFindState: FindState = { results: {}, active: null, review: null };

export type FindAction =
  | { type: "begin"; ids: number[]; many: boolean }
  | { type: "attach"; search: number }
  | { type: "failed" }
  | { type: "event"; event: FindEvent }
  /** The engine's kept results, after a search ended (or on load); `finished` ends the search. */
  | {
      type: "synced";
      results: FindResult[];
      finished: Extract<FindEvent, { kind: "finished" }> | null;
    }
  | { type: "skip"; id: number }
  | { type: "closeReview" }
  /** The ids of the downloads still missing: results of any other are dropped. */
  | { type: "prune"; missing: ReadonlySet<number> };

/** Whether `event` belongs to the search the window is following. */
function ours(active: ActiveSearch | null, search: number): active is ActiveSearch {
  return active != null && (active.search == null || active.search === search);
}

export function findReducer(state: FindState, action: FindAction): FindState {
  switch (action.type) {
    case "begin": {
      const results = { ...state.results };
      for (const id of action.ids) delete results[id];
      return {
        results,
        review: null,
        active: {
          search: null,
          ids: action.ids,
          many: action.many,
          foldersDone: 0,
          foldersTotal: 0,
          current: null,
        },
      };
    }
    case "attach":
      return state.active && state.active.search == null
        ? { ...state, active: { ...state.active, search: action.search } }
        : state;
    case "failed":
      return { ...state, active: null };
    case "event": {
      const { event } = action;
      if (!ours(state.active, event.search)) return state;
      const active = { ...state.active, search: event.search };
      if (event.kind === "progress") {
        return {
          ...state,
          active: {
            ...active,
            foldersDone: event.folders_done,
            foldersTotal: event.folders_total,
            current: event.current,
          },
        };
      }
      if (event.kind === "result") {
        return { ...state, active, results: { ...state.results, [event.result.id]: event.result } };
      }
      return state;
    }
    case "synced": {
      const results = { ...state.results };
      for (const r of action.results) results[r.id] = r;
      const { finished } = action;
      if (!finished || !ours(state.active, finished.search)) return { ...state, results };
      const review = state.active.many && !finished.cancelled ? state.active.ids : null;
      return { results, active: null, review };
    }
    case "skip": {
      const results = { ...state.results };
      delete results[action.id];
      return { ...state, results };
    }
    case "closeReview":
      return state.review ? { ...state, review: null } : state;
    case "prune": {
      const keep = Object.keys(state.results).filter((id) => action.missing.has(Number(id)));
      const pruned = keep.length !== Object.keys(state.results).length;
      const review = state.review?.filter((id) => action.missing.has(id)) ?? null;
      const reviewChanged = review != null && review.length !== state.review?.length;
      if (!pruned && !reviewChanged) return state;
      return {
        ...state,
        results: Object.fromEntries(keep.map((id) => [id, state.results[Number(id)]])),
        review: review && review.length > 0 ? review : null,
      };
    }
  }
}

/** The ids of the downloads whose files are missing, as the engine reports them. */
export const missingIds = (downloads: readonly DownloadSnapshot[]): number[] =>
  downloads.filter(isMissing).map((d) => d.id);

/** Whether a many-file search is the one running (Find files; its bar shows). */
export const searchingMany = (s: FindState): ActiveSearch | null =>
  s.active?.many ? s.active : null;

/** The search running for exactly this one download, if it is a Find file. */
export const searchingOne = (s: FindState, id: number): ActiveSearch | null =>
  s.active && !s.active.many && s.active.ids.includes(id) ? s.active : null;

/** A match is certain enough to start ticked: the same name and size. Renamed ones start unticked. */
export const startsTicked = (m: FoundFile): boolean => m.reason === "SameNameAndSize";

/** The rows of a review: each reviewed download still missing, with its result if it has one. */
export interface ReviewRow {
  d: DownloadSnapshot;
  result: FindResult | undefined;
  /** The best match (the engine lists the best folder first), if any. */
  match: FoundFile | undefined;
}

export function reviewRows(
  review: readonly number[],
  downloads: readonly DownloadSnapshot[],
  results: Readonly<Record<number, FindResult>>,
): ReviewRow[] {
  return review.flatMap((id) => {
    const d = downloads.find((x) => x.id === id);
    if (!d || !isMissing(d)) return [];
    const result = results[id];
    return [{ d, result, match: result?.matches[0] }];
  });
}

/** The ids ticked when a review opens. */
export const initialTicks = (rows: readonly ReviewRow[]): Set<number> =>
  new Set(rows.filter((r) => r.match && startsTicked(r.match)).map((r) => r.d.id));

const key = (p: string) =>
  p
    .replace(/[\\/]+$/, "")
    .replace(/\//g, "\\")
    .toLowerCase();

/** The one folder every found file is in, if they share one (for "Found in E:\Archive"). */
export function sharedFolder(rows: readonly ReviewRow[]): string | null {
  const dirs = rows.flatMap((r) => (r.match ? [folderOf(r.match.path)] : []));
  if (dirs.length === 0) return null;
  return dirs.every((d) => key(d) === key(dirs[0])) ? dirs[0] : null;
}

/** The missing downloads whose old folder is `folder`: the ones a same-folder offer is about. */
export function inOldFolder(downloads: readonly DownloadSnapshot[], folder: string): number[] {
  return downloads
    .filter(
      (d) => isMissing(d) && d.final_path != null && key(folderOf(d.final_path)) === key(folder),
    )
    .map((d) => d.id);
}
