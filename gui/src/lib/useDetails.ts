import { useCallback, useEffect, useState, type RefObject } from "react";
import type { Modifiers } from "./selection";
import type { DownloadSnapshot } from "./types";

type Deps = {
  downloads: readonly DownloadSnapshot[];
  seeded: boolean;
  /** The selection's click: selects a row (alone, with no modifiers). */
  selectRow: (id: number, mods: Modifiers) => void;
  tableRef: RefObject<HTMLTableElement | null>;
};

/** The details pane's row: which one it shows, and opening and closing it. */
export function useDetails({ downloads, seeded, selectRow, tableRef }: Deps) {
  // The row in the details pane. Kept apart from the selection so the pane stays
  // when its row leaves the view (it finishes, or a search hides it).
  const [detailsId, setDetailsId] = useState<number | null>(null);
  // The pane follows the live row and closes once it's gone (removed).
  const detailsRow = downloads.find((d) => d.id === detailsId);
  useEffect(() => {
    if (detailsId != null && seeded && !detailsRow) setDetailsId(null);
  }, [detailsId, seeded, detailsRow]);

  // The filename button toggles the row's details. A mouse press hands focus to
  // the table so the keyboard map works; a key press leaves it on the button.
  const onDetails = useCallback(
    (id: number, viaPointer: boolean) => {
      if (detailsId === id) {
        setDetailsId(null);
      } else {
        selectRow(id, {});
        setDetailsId(id);
      }
      if (viaPointer) tableRef.current?.focus({ preventScroll: true });
    },
    [detailsId, selectRow],
  );

  // Closing the pane from inside it hands focus back to the row that opened it.
  const closeDetails = useCallback(() => {
    const id = detailsId;
    const hadFocus = document.activeElement?.closest('[data-testid="details-pane"]') != null;
    setDetailsId(null);
    if (!hadFocus || id == null) return;
    setTimeout(() => {
      const button = document.querySelector<HTMLButtonElement>(
        `[data-testid="download-row"][data-id="${id}"] .fn-btn`,
      );
      (button ?? tableRef.current)?.focus();
    });
  }, [detailsId]);

  return { detailsId, setDetailsId, detailsRow, onDetails, closeDetails };
}
