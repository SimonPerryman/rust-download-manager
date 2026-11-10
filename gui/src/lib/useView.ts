import { useEffect, useMemo, useState } from "react";
import { viewTitle } from "./copy";
import {
  archivedCount,
  isArchivedView,
  matchesFilter,
  matchesQuery,
  statusCounts,
  type ViewFilter,
} from "./filters";
import { deriveFolders, folderOf } from "./folders";
import { arrangeRows, DEFAULT_SORT, isVolatile, nextSort, rankOf, type SortKey } from "./sort";
import { hasTag, tagCounts } from "./tags";
import type { DownloadSnapshot } from "./types";

const ALL: ViewFilter = { kind: "status", status: "all" };

/**
 * The table's view: the sidebar filter, the search and the sort, and the rows,
 * counts, folders and tags they give.
 */
export function useView(downloads: DownloadSnapshot[]) {
  const [filter, setFilter] = useState<ViewFilter>(ALL);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState(DEFAULT_SORT);
  const [frozen, setFrozen] = useState<Map<number, number> | null>(null);

  // The working views and their counts leave archived rows out.
  const live = useMemo(() => downloads.filter((d) => !d.archived), [downloads]);
  const counts = useMemo(() => statusCounts(downloads), [downloads]);
  const folders = useMemo(() => deriveFolders(live), [live]);
  const archived = archivedCount(downloads);

  // A folder or tag view falls back once nothing in its scope is left (an
  // archived tag to Archived, anything else to All).
  const viewGone =
    (filter.kind === "folder" && !live.some((d) => folderOf(d.target) === filter.dir)) ||
    (filter.kind === "tag" &&
      !downloads.some((d) => d.archived === filter.archived && hasTag(d, filter.tag)));
  const fallback: ViewFilter =
    filter.kind === "tag" && filter.archived ? { kind: "archived" } : ALL;
  const view = viewGone ? fallback : filter;
  useEffect(() => {
    if (viewGone) setFilter(fallback);
  }, [viewGone, fallback]);
  const inArchive = isArchivedView(view);
  const tags = useMemo(
    () => tagCounts(downloads.filter((d) => d.archived === inArchive)),
    [downloads, inArchive],
  );

  const visible = useMemo(
    () =>
      arrangeRows(
        downloads.filter((d) => matchesFilter(d, view) && matchesQuery(d, query)),
        { sort, frozen, pinFailed: view.kind === "status" && view.status === "all" },
      ),
    [downloads, view, query, sort, frozen],
  );
  // Retry all failed leaves archived rows alone, as the engine command does.
  const failed = useMemo(() => live.filter((d) => d.status === "failed"), [live]);
  const title = viewTitle(view, folders);

  const onSort = (key: SortKey) => {
    const next = nextSort(sort, key);
    setSort(next);
    // Live values would reshuffle rows under the pointer, so freeze the order they had.
    setFrozen(isVolatile(next.key) ? rankOf(downloads, next) : null);
  };

  return {
    setFilter,
    query,
    setQuery,
    sort,
    onSort,
    live,
    counts,
    folders,
    archived,
    view,
    inArchive,
    tags,
    visible,
    failed,
    title,
  };
}
