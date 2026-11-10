import { useCallback, useMemo, useState, type RefObject } from "react";
import * as api from "./api";
import { focusIsFree } from "./focus";
import { keyCommand } from "./keymap";
import { menuItems, type MenuCommand, type MenuEntry } from "./menu";
import { dropIndex, stepIndex } from "./queue";
import { continues, isPausable } from "./rowView";
import { selectedInOrder, type Modifiers } from "./selection";
import type { DownloadSnapshot } from "./types";
import type { SelectionApi } from "./useSelection";

/** The keyboard opens the menu this far in from the focused row's left edge. */
const MENU_INSET_PX = 48;

const scrollToRow = (id: number) =>
  document.getElementById(`row-${id}`)?.scrollIntoView({ block: "nearest" });

type Deps = {
  selection: SelectionApi;
  /** The rows on show, in display order. */
  visibleIds: readonly number[];
  byId: (id: number) => DownloadSnapshot | undefined;
  /** The waiting order, first to start first. */
  queue: readonly number[];
  failed: readonly DownloadSnapshot[];
  inArchive: boolean;
  run: (cmd: MenuCommand, ids: readonly number[]) => void;
  openDownload: (d: DownloadSnapshot) => void;
  detailsId: number | null;
  setDetailsId: (id: number | null) => void;
  fail: (e: unknown) => void;
  tableRef: RefObject<HTMLTableElement | null>;
};

/** The table's selection, row menu and keyboard, and moves in the queue. */
export function useTableInput({
  selection,
  visibleIds,
  byId,
  queue,
  failed,
  inArchive,
  run,
  openDownload,
  detailsId,
  setDetailsId,
  fail,
  tableRef,
}: Deps) {
  // The menu's items are fixed when it opens, so a row changing under it can't
  // unmount the focused item; each command still acts only where it applies.
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    ids: number[];
    entries: MenuEntry[];
  } | null>(null);
  const sel = selection.selection;

  const selectedIds = useMemo(() => selectedInOrder(sel, visibleIds), [sel, visibleIds]);
  const selectedRows = useMemo(
    () => selectedIds.flatMap((id) => byId(id) ?? []),
    [selectedIds, byId],
  );
  const { click: selectRow, clear: clearSelection } = selection;

  // A plain click selects the row alone and shows its details; Ctrl and Shift only select.
  const onSelect = useCallback(
    (id: number, mods: Modifiers) => {
      selectRow(id, mods);
      if (!mods.ctrl && !mods.shift) setDetailsId(id);
      tableRef.current?.focus({ preventScroll: true });
    },
    [selectRow],
  );

  // The menu acts on the selection when opened on a selected row, else selects that row alone.
  const openMenu = useCallback(
    (id: number, x: number, y: number) => {
      const onSelected = sel.selected.has(id);
      if (!onSelected) selectRow(id, {});
      const ids = onSelected ? selectedIds : [id];
      const rows = ids.flatMap((i) => byId(i) ?? []);
      if (rows.length) {
        setMenu({ x, y, ids, entries: menuItems(rows, queue, failed.length, inArchive) });
      }
    },
    [sel, selectedIds, selectRow, byId, queue, failed.length, inArchive],
  );

  const closeMenu = useCallback(() => {
    setMenu(null);
    setTimeout(() => focusIsFree() && tableRef.current?.focus({ preventScroll: true }));
  }, []);

  const runMenu = (cmd: MenuCommand) => {
    const ids = menu?.ids ?? [];
    closeMenu();
    run(cmd, ids);
  };

  const moveInQueue = useCallback(
    (id: number, index: number | null) => {
      if (index != null) api.moveQueued(id, index).catch(fail);
    },
    [fail],
  );

  const onTableKey = (e: React.KeyboardEvent<HTMLTableElement>) => {
    // Keys on a row's own buttons and editors are theirs.
    if (e.target !== e.currentTarget) return;
    const c = keyCommand(e);
    if (!c) return;
    const cursor = sel.cursor != null ? byId(sel.cursor) : undefined;
    switch (c.kind) {
      case "move": {
        e.preventDefault();
        const id = selection.move(c.to, c.extend);
        if (id != null) scrollToRow(id);
        // An open pane follows the cursor.
        if (id != null && !c.extend && detailsId != null) setDetailsId(id);
        return;
      }
      case "selectAll":
        e.preventDefault();
        return selection.selectAll();
      case "pauseOrResume":
        e.preventDefault();
        if (selectedRows.some(isPausable)) return run("pause", selectedIds);
        // Space never restarts a stopped download from zero; the menu's "Download again" does.
        if (selectedRows.some(continues))
          return run(
            "resume",
            selectedIds.filter((id) => continues(byId(id)!)),
          );
        return;
      case "open":
        if (cursor?.status === "completed") openDownload(cursor);
        return;
      case "remove":
        // Del archives; in the archive it removes for good.
        return run(inArchive ? "remove" : "archive", selectedIds);
      case "menu": {
        e.preventDefault();
        const row = cursor && document.getElementById(`row-${cursor.id}`);
        if (!cursor || !row) return;
        const r = row.getBoundingClientRect();
        openMenu(cursor.id, r.left + MENU_INSET_PX, r.bottom);
        return;
      }
      case "reorder":
        e.preventDefault();
        if (cursor?.status === "queued") {
          moveInQueue(cursor.id, stepIndex(queue, cursor.id, c.delta));
        }
        return;
      case "clear":
        if (sel.selected.size || detailsId != null) {
          e.preventDefault();
          clearSelection();
          setDetailsId(null);
        }
        return;
    }
  };

  const onQueueDrop = useCallback(
    (id: number, target: number, after: boolean) =>
      moveInQueue(id, dropIndex(queue, id, target, after)),
    [moveInQueue, queue],
  );

  return {
    menu,
    setMenu,
    selectedIds,
    selectedRows,
    clearSelection,
    onSelect,
    openMenu,
    closeMenu,
    runMenu,
    moveInQueue,
    onTableKey,
    onQueueDrop,
  };
}
