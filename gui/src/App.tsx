import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "./lib/api";
import {
  addedMessage,
  deletedPartialMessage,
  emptyCopy,
  openFailedMessage,
  removedManyMessage,
  removedMessage,
  retryAllMessage,
  stoppedManyMessage,
  stoppedMessage,
  viewTitle,
} from "./lib/copy";
import { finishedToArchive } from "./lib/archive";
import { knownUrls } from "./lib/batch";
import {
  archivedCount,
  isArchivedView,
  matchesFilter,
  matchesQuery,
  statusCounts,
  type ViewFilter,
} from "./lib/filters";
import { deriveFolders, folderOf } from "./lib/folders";
import { focusIsFree } from "./lib/focus";
import { keyCommand } from "./lib/keymap";
import { menuItems, selectionActions, type MenuCommand, type MenuEntry } from "./lib/menu";
import { loadConfirmRemove, saveConfirmRemove } from "./lib/prefs";
import { dropIndex, queueOrder, stepIndex } from "./lib/queue";
import {
  continues,
  isPausable,
  isRemovable,
  isStoppable,
  pauseAllMode,
  totalSpeed,
} from "./lib/rowView";
import { selectedInOrder, type Modifiers } from "./lib/selection";
import { arrangeRows, DEFAULT_SORT, isVolatile, nextSort, rankOf, type SortKey } from "./lib/sort";
import { hasTag, tagCounts } from "./lib/tags";
import type { DownloadSnapshot } from "./lib/types";
import { useArchive } from "./lib/useArchive";
import { useBulkCommands } from "./lib/useBulkCommands";
import { useDownloads } from "./lib/useDownloads";
import { useFinishMoment } from "./lib/useFinishMoment";
import { usePrefs } from "./lib/usePrefs";
import { usePaneHeight } from "./lib/usePaneHeight";
import { useSelection } from "./lib/useSelection";
import { useSettings } from "./lib/useSettings";
import { useSpeedHistory } from "./lib/useSpeedHistory";
import { useToast } from "./lib/useToast";
import { useUrlIntake } from "./lib/useUrlIntake";
import { DetailsPane } from "./components/details/DetailsPane";
import type { PaneTab } from "./components/details/tabs";
import { AddDialog } from "./components/dialogs/AddDialog";
import { RemoveDialog } from "./components/dialogs/RemoveDialog";
import { TagsDialog } from "./components/dialogs/TagsDialog";
import { StopDialog } from "./components/dialogs/StopDialog";
import { DropOverlay } from "./components/DropOverlay";
import { FinishNotifications } from "./components/feedback/FinishNotifications";
import { Toasts } from "./components/feedback/Toasts";
import { Sidebar } from "./components/sidebar/Sidebar";
import { StatusBar } from "./components/statusbar/StatusBar";
import type { RowHandlers } from "./components/table/rowHandlers";
import { DownloadTable } from "./components/table/DownloadTable";
import { RowMenu } from "./components/table/RowMenu";
import { Toolbar } from "./components/toolbar/Toolbar";

const ALL: ViewFilter = { kind: "status", status: "all" };

// How long a confirmed stop may take to land before focus gives up on the row.
const REFOCUS_MS = 2_000;

/** The keyboard opens the menu this far in from the focused row's left edge. */
const MENU_INSET_PX = 48;

const scrollToRow = (id: number) =>
  document.getElementById(`row-${id}`)?.scrollIntoView({ block: "nearest" });

// State wiring only: every download fact comes from the engine's snapshots,
// and every action is a thin api call.
export default function App() {
  const { downloads, seeded } = useDownloads();
  const [prefs, updatePrefs, resolved] = usePrefs();
  const { settings, apply: applySettings } = useSettings();
  const { toast, show: notify, dismiss: dismissToast } = useToast();
  const finish = useFinishMoment(downloads, seeded);
  const archiver = useArchive(notify);
  const history = useSpeedHistory(downloads);
  const pane = usePaneHeight();

  const [filter, setFilter] = useState<ViewFilter>(ALL);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState(DEFAULT_SORT);
  const [frozen, setFrozen] = useState<Map<number, number> | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [prefillText, setPrefillText] = useState("");
  const [stopIds, setStopIds] = useState<number[] | null>(null);
  const [removeRows, setRemoveRows] = useState<DownloadSnapshot[] | null>(null);
  const [tagIds, setTagIds] = useState<number[] | null>(null);
  const [confirmRemove, setConfirmRemove] = useState(loadConfirmRemove);
  const [refocusId, setRefocusId] = useState<number | null>(null);
  // The row in the details pane. Kept apart from the selection so the pane stays
  // when its row leaves the view (it finishes, or a search hides it).
  const [detailsId, setDetailsId] = useState<number | null>(null);
  const [paneTab, setPaneTab] = useState<PaneTab>("overview");
  // The menu's items are fixed when it opens, so a row changing under it can't
  // unmount the focused item; each command still acts only where it applies.
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    ids: number[];
    entries: MenuEntry[];
  } | null>(null);
  const tableRef = useRef<HTMLTableElement>(null);

  const openAdd = useCallback((text = "") => {
    setMenu(null);
    setPrefillText(text);
    setAddOpen(true);
  }, []);
  const draggingUrl = useUrlIntake(
    openAdd,
    !addOpen && stopIds == null && removeRows == null && tagIds == null,
  );

  // The working views and their counts leave archived rows out.
  const live = useMemo(() => downloads.filter((d) => !d.archived), [downloads]);
  const counts = useMemo(() => statusCounts(downloads), [downloads]);
  const folders = useMemo(() => deriveFolders(live, live), [live]);
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
  const visibleIds = useMemo(() => visible.map((d) => d.id), [visible]);
  const selection = useSelection(visibleIds);
  const sel = selection.selection;
  const queue = useMemo(() => queueOrder(downloads), [downloads]);
  // Keyed on the URLs themselves so the Add preview doesn't re-parse on every tick.
  const urlKey = JSON.stringify(downloads.map((d) => [d.url, d.archived]));
  const existingUrls = useMemo(
    () =>
      knownUrls(
        (JSON.parse(urlKey) as [string, boolean][]).map(([url, archived]) => ({ url, archived })),
      ),
    [urlKey],
  );
  const byId = useCallback((id: number) => downloads.find((d) => d.id === id), [downloads]);
  // The pane follows the live row and closes once it's gone (removed).
  const detailsRow = downloads.find((d) => d.id === detailsId);
  useEffect(() => {
    if (detailsId != null && seeded && !detailsRow) setDetailsId(null);
  }, [detailsId, seeded, detailsRow]);
  const title = viewTitle(view, folders);

  const onSort = (key: SortKey) => {
    const next = nextSort(sort, key);
    setSort(next);
    // Live values would reshuffle rows under the pointer, so freeze the order they had.
    setFrozen(isVolatile(next.key) ? rankOf(downloads, next) : null);
  };

  const fail = useCallback((e: unknown) => notify(String(e)), [notify]);
  const { markOpened } = finish;

  // ---- remove (for good, from the archive) and tags ----
  const removeNow = useCallback(
    (rows: readonly DownloadSnapshot[]) => {
      Promise.all(rows.map((d) => api.remove(d.id)))
        .then(() => notify(rows.length === 1 ? removedMessage(rows[0]) : removedManyMessage(rows)))
        .catch(fail);
    },
    [notify, fail],
  );
  const askRemove = useCallback(
    (rows: DownloadSnapshot[]) => (confirmRemove ? setRemoveRows(rows) : removeNow(rows)),
    [confirmRemove, removeNow],
  );
  const updateConfirmRemove = useCallback((ask: boolean) => {
    setConfirmRemove(ask);
    saveConfirmRemove(ask);
  }, []);
  const cancelRemove = useCallback(() => {
    setRemoveRows(null);
    setTimeout(() => focusIsFree() && tableRef.current?.focus({ preventScroll: true }));
  }, []);
  const confirmRemoveRows = (dontAsk: boolean) => {
    // Only what's still archived and finished: a row resumed or unarchived
    // while the dialog was open stays.
    const rows = (removeRows ?? []).flatMap((r) => {
      const d = byId(r.id);
      return d && d.archived && isRemovable(d) ? [d] : [];
    });
    setRemoveRows(null);
    if (dontAsk) updateConfirmRemove(false);
    removeNow(rows);
    setTimeout(() => focusIsFree() && tableRef.current?.focus({ preventScroll: true }));
  };
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

  const openDownload = useCallback(
    (d: DownloadSnapshot) => {
      api
        .openDownload(d.id)
        .then(() => markOpened(d.id))
        .catch((e) => notify(openFailedMessage(d, e)));
    },
    [markOpened, notify],
  );
  const revealDownload = useCallback(
    (d: DownloadSnapshot) => {
      api.revealDownload(d.id).catch(fail);
    },
    [fail],
  );

  const retryAll = useCallback(() => {
    const message = retryAllMessage(failed);
    api
      .retryFailed()
      .then(() => notify(message))
      .catch(fail);
  }, [failed, notify, fail]);

  const handlers = useMemo<RowHandlers>(
    () => ({
      pause: (d) => api.pause(d.id).catch(fail),
      resume: (d) => api.resume(d.id).catch(fail),
      stop: (d) => setStopIds([d.id]),
      open: openDownload,
      reveal: revealDownload,
      remove: (d) => askRemove([d]),
      archive: (d) => archiver.archive([d]),
      deletePartial: (d) =>
        api
          .stop(d.id, false)
          .then(() => notify(deletedPartialMessage(d)))
          .catch(fail),
      retryAll,
    }),
    [fail, notify, openDownload, revealDownload, retryAll, askRemove, archiver.archive],
  );

  const run = useBulkCommands({
    downloads,
    queue,
    notify,
    fail,
    open: openDownload,
    reveal: revealDownload,
    retryAll,
    askStop: setStopIds,
    askRemove,
    askTags: setTagIds,
    archive: archiver.archive,
  });

  // The dialog follows the live rows and closes itself once none can be stopped.
  const stopRows = useMemo(() => (stopIds ?? []).map(byId).filter(isStoppable), [stopIds, byId]);
  const stopOpen = stopRows.length > 0;
  useEffect(() => {
    if (stopIds == null || stopOpen) return;
    const id = stopIds.length === 1 ? stopIds[0] : null;
    setStopIds(null);
    // The rows finished or failed under the dialog: give focus back to the row (or the table).
    setTimeout(() => {
      if (!focusIsFree()) return;
      const button =
        id != null
          ? document.querySelector<HTMLButtonElement>(
              `[data-testid="download-row"][data-id="${id}"] .c-act button`,
            )
          : null;
      (button ?? tableRef.current)?.focus();
    });
  }, [stopIds, stopOpen]);

  const cancelStop = useCallback(() => {
    const ids = stopIds;
    setStopIds(null);
    if (ids == null) return;
    setTimeout(() => {
      const button =
        ids.length === 1
          ? document.querySelector<HTMLButtonElement>(
              `[data-testid="download-row"][data-id="${ids[0]}"] button[aria-label="Stop"]`,
            )
          : null;
      (button ?? tableRef.current)?.focus();
    });
  }, [stopIds]);

  const confirmStop = (keep: boolean) => {
    const rows = stopRows;
    setStopIds(null);
    if (rows.length === 0) return;
    Promise.all(rows.map((d) => api.stop(d.id, keep)))
      .then(() => {
        if (rows.length === 1) {
          notify(stoppedMessage(rows[0], keep));
          setRefocusId(rows[0].id);
        } else {
          notify(stoppedManyMessage(rows.length, keep));
          setTimeout(() => focusIsFree() && tableRef.current?.focus());
        }
      })
      .catch(fail);
  };

  // ---- selection, menu and keyboard ----
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

  // If the stopped row never shows up as Stopped here (filtered out, say), focus the table.
  useEffect(() => {
    if (refocusId == null) return;
    const t = setTimeout(() => {
      if (focusIsFree()) tableRef.current?.focus();
      setRefocusId(null);
    }, REFOCUS_MS);
    return () => clearTimeout(t);
  }, [refocusId]);
  const onRefocused = useCallback(() => setRefocusId(null), []);

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

  return (
    <div className="app">
      <Sidebar
        filter={view}
        onFilter={setFilter}
        counts={counts}
        folders={folders}
        archivedCount={archived}
        tags={tags}
        bumpSeq={finish.bumpSeq}
        speed={totalSpeed(downloads)}
        speedSamples={history.total}
        globalLimit={settings?.global_limit ?? null}
      />
      <main className="main">
        <Toolbar
          title={title}
          count={visible.length}
          onRetryAll={
            view.kind === "status" && view.status === "failed" && failed.length > 0
              ? retryAll
              : null
          }
          query={query}
          onQuery={setQuery}
          pauseMode={pauseAllMode(downloads)}
          onPauseAll={() => api.pauseAll().catch(fail)}
          onResumeAll={() => api.resumeAll().catch(fail)}
          onArchiveFinished={inArchive ? null : () => archiver.archiveFinished(live)}
          canArchiveFinished={finishedToArchive(live).length > 0}
          archived={
            inArchive
              ? {
                  canAct: selectedIds.length > 0,
                  onUnarchive: () => run("unarchive", selectedIds),
                  onRemove: () => run("remove", selectedIds),
                }
              : null
          }
          confirmRemove={confirmRemove}
          onConfirmRemove={updateConfirmRemove}
          onAdd={() => openAdd()}
          prefs={prefs}
          resolved={resolved}
          onPrefs={updatePrefs}
          onSettingsApplied={applySettings}
          selection={
            selectedRows.length >= 2
              ? { count: selectedRows.length, actions: selectionActions(selectedRows, inArchive) }
              : null
          }
          onSelectionCommand={(cmd) => {
            run(cmd, selectedIds);
            // The clicked button may go (Pause turns into Resume, Remove ends the selection).
            setTimeout(() => focusIsFree() && tableRef.current?.focus({ preventScroll: true }));
          }}
          onClearSelection={() => {
            clearSelection();
            setDetailsId(null);
            tableRef.current?.focus({ preventScroll: true });
          }}
        />
        <DownloadTable
          rows={visible}
          sort={sort}
          onSort={onSort}
          unopened={finish.unopened}
          fresh={finish.fresh}
          failedCount={failed.length}
          handlers={handlers}
          refocusId={refocusId}
          onRefocused={onRefocused}
          empty={emptyCopy(view, title, query)}
          onAdd={() => openAdd()}
          tableRef={tableRef}
          selection={sel}
          queue={queue}
          queueView={view.kind === "status" && view.status === "waiting"}
          onSelect={onSelect}
          onMenu={openMenu}
          onKeyDown={onTableKey}
          onQueueDrop={onQueueDrop}
          detailsId={detailsRow?.id ?? null}
          onDetails={onDetails}
        />
        {detailsRow && (
          <DetailsPane
            d={detailsRow}
            tab={paneTab}
            onTab={setPaneTab}
            height={pane.height}
            maxHeight={pane.maxHeight}
            onResize={pane.resize}
            onClose={closeDetails}
            samples={history.byId.get(detailsRow.id) ?? []}
            globalLimit={settings?.global_limit ?? null}
            onReveal={revealDownload}
            spot={
              detailsRow.status === "queued" && queue.includes(detailsRow.id)
                ? { index: queue.indexOf(detailsRow.id), count: queue.length }
                : null
            }
            onMove={(delta) => moveInQueue(detailsRow.id, stepIndex(queue, detailsRow.id, delta))}
          />
        )}
      </main>
      <StatusBar
        speed={totalSpeed(downloads)}
        downloading={counts.downloading}
        waiting={counts.waiting}
        settings={settings}
        applySettings={applySettings}
      />

      {stopOpen && <StopDialog ds={stopRows} onCancel={cancelStop} onConfirm={confirmStop} />}
      {menu && (
        <RowMenu entries={menu.entries} x={menu.x} y={menu.y} onRun={runMenu} onClose={closeMenu} />
      )}
      <AddDialog
        open={addOpen}
        initialText={prefillText}
        existingUrls={existingUrls}
        onClose={() => setAddOpen(false)}
        onAdded={(n) => n > 1 && notify(addedMessage(n))}
      />
      {removeRows && (
        <RemoveDialog ds={removeRows} onCancel={cancelRemove} onConfirm={confirmRemoveRows} />
      )}
      {tagRows.length > 0 && <TagsDialog ds={tagRows} onClose={closeTags} />}
      <Toasts
        pending={archiver.pending}
        onUndo={archiver.undo}
        toast={toast}
        onDismissToast={dismissToast}
      />
      <FinishNotifications
        notifications={finish.notifications}
        onOpen={(d) => openDownload(downloads.find((x) => x.id === d.id) ?? d)}
        onReveal={(d) => revealDownload(downloads.find((x) => x.id === d.id) ?? d)}
        onDismiss={finish.dismissNotification}
      />
      {draggingUrl && <DropOverlay />}
    </div>
  );
}
