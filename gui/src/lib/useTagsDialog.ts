import { useCallback, useEffect, useMemo, useState, type RefObject } from "react";
import { focusIsFree } from "./focus";
import type { DownloadSnapshot } from "./types";

type Deps = {
  downloads: readonly DownloadSnapshot[];
  seeded: boolean;
  tableRef: RefObject<HTMLTableElement | null>;
};

/** The Tags dialog: which rows it edits, and closing it. */
export function useTagsDialog({ downloads, seeded, tableRef }: Deps) {
  const [tagIds, setTagIds] = useState<number[] | null>(null);
  // The Tags dialog follows the live rows and closes itself once none is left.
  const tagRows = useMemo(
    () => (tagIds ?? []).flatMap((id) => downloads.find((d) => d.id === id) ?? []),
    [tagIds, downloads],
  );
  const closeTags = useCallback(() => {
    setTagIds(null);
    setTimeout(() => focusIsFree() && tableRef.current?.focus({ preventScroll: true }));
  }, []);
  useEffect(() => {
    if (tagIds != null && seeded && tagRows.length === 0) setTagIds(null);
  }, [tagIds, seeded, tagRows.length]);

  return { tagIds, setTagIds, tagRows, closeTags };
}
