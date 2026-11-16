import { useEffect, useMemo, useState } from "react";
import { batchGroups, layoutRows } from "./batches";
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
export function useView(downloads: DownloadSnapshot[], groupBatches: boolean) {
  const [filter, setFilter] = useState<ViewFilter>(ALL);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState(DEFAULT_SORT);
  const [frozen, setFrozen] = useState<Map<number, number> | null>(null);
  const [collapsed, setCollapsed] = useState<ReadonlySet<number>>(new Set());

  // The working views and their counts leave archived rows out.
  const live = useMemo(() => downloads.filter((d) => !d.archived), [downloads]);
  const counts = useMemo(() => statusCounts(downloads), [downloads]);
  const folders = useMemo(() => deriveFolders(live), [live]);
  const archived = archivedCount(downloads);
  const batches = useMemo(() => batchGroups(downloads), [downloads]);

  // A folder or tag view falls back once nothing in its scope is left (an
  // archived tag to Archived, anything else to All).
  const viewGone =
    (filter.kind === "folder" && !live.some((d) => folderOf(d.target) === filter.dir)) ||
    (filter.kind === "batch" && !batches.some((b) => b.id === filter.batch)) ||
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

  const arranged = useMemo(
    () =>
      arrangeRows(
        downloads.filter((d) => matchesFilter(d, view) && matchesQuery(d, query)),
        { sort, frozen, pinFailed: view.kind === "status" && view.status === "all" },
      ),
    [downloads, view, query, sort, frozen],
  );
  // A batch's files gather under its group row in the working views, except in the
  // batch's own view, which is that group already.
  const grouped = groupBatches && !inArchive && view.kind !== "batch";
  const { items, shown: visible } = useMemo(
    () => layoutRows(arranged, batches, grouped, collapsed),
    [arranged, batches, grouped, collapsed],
  );
  const toggleBatch = (id: number) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  // Retry all failed leaves archived rows alone, as the engine command does.
  const failed = useMemo(() => live.filter((d) => d.status === "failed"), [live]);
  const title = viewTitle(view, folders, batches);

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
    batches,
    items,
    toggleBatch,
    visible,
    failed,
    title,
  };
}
