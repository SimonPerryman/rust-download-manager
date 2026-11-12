import { useCallback, useMemo, useRef, useState } from "react";
import * as api from "./lib/api";
import {
  addedMessage,
  deletedPartialMessage,
  emptyCopy,
  openFailedMessage,
  removeSelectedTitle,
  retryAllMessage,
} from "./lib/copy";
import { finishedToArchive, toRemove } from "./lib/archive";
import { knownUrls } from "./lib/batch";
import { focusIsFree } from "./lib/focus";
import { EMPTY_WORK } from "./lib/laneWork";
import { selectionActions } from "./lib/menu";
import { queueOrder, stepIndex } from "./lib/queue";
import { pauseAllMode, totalSpeed } from "./lib/rowView";
import type { DownloadSnapshot } from "./lib/types";
import { useArchive } from "./lib/useArchive";
import { useBulkCommands } from "./lib/useBulkCommands";
import { useDetails } from "./lib/useDetails";
import { useDownloads } from "./lib/useDownloads";
import { useFinishMoment } from "./lib/useFinishMoment";
import { useLaneWork } from "./lib/useLaneWork";
import { usePrefs } from "./lib/usePrefs";
import { usePaneHeight } from "./lib/usePaneHeight";
import { useRemoveDialog } from "./lib/useRemoveDialog";
import { useSelection } from "./lib/useSelection";
import { useSettings } from "./lib/useSettings";
import { useSpeedHistory } from "./lib/useSpeedHistory";
import { useStopDialog } from "./lib/useStopDialog";
import { useTableInput } from "./lib/useTableInput";
import { useTagsDialog } from "./lib/useTagsDialog";
import { useToast } from "./lib/useToast";
import { pausedBarCount, updatedHeading, willPauseCount } from "./lib/updater";
import { useUpdater } from "./lib/useUpdater";
import { useUrlIntake } from "./lib/useUrlIntake";
import { useView } from "./lib/useView";
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
import { FirstRunCard } from "./components/updater/FirstRunCard";
import { PausedForUpdateBar } from "./components/updater/PausedForUpdateBar";
import { UpdateNotice } from "./components/updater/UpdateNotice";

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
  const laneWork = useLaneWork(downloads);
  const pane = usePaneHeight();
  const [updates, updater] = useUpdater();

  const {
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
  } = useView(downloads);

  const [addOpen, setAddOpen] = useState(false);
  const [prefillText, setPrefillText] = useState("");
  const [paneTab, setPaneTab] = useState<PaneTab>("overview");
  const tableRef = useRef<HTMLTableElement>(null);

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
  const { detailsId, setDetailsId, detailsRow, onDetails, closeDetails } = useDetails({
    downloads,
    seeded,
    selectRow: selection.click,
    tableRef,
  });

  const fail = useCallback((e: unknown) => notify(String(e)), [notify]);
  const { markOpened } = finish;

  const {
    removeIds,
    askRemove,
    confirmRemove,
    updateConfirmRemove,
    removeRows,
    removeOpen,
    closeRemove,
    confirmRemoveRows,
  } = useRemoveDialog({ byId, seeded, notify, fail, tableRef });
  const { tagIds, setTagIds, tagRows, closeTags } = useTagsDialog({ downloads, seeded, tableRef });

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

  const {
    stopIds,
    setStopIds,
    stopRows,
    stopOpen,
    cancelStop,
    confirmStop,
    refocusId,
    onRefocused,
  } = useStopDialog({ byId, notify, fail, tableRef });

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

  const {
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
  } = useTableInput({
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
  });

  const openAdd = useCallback((text = "") => {
    setMenu(null);
    setPrefillText(text);
    setAddOpen(true);
  }, []);
  const draggingUrl = useUrlIntake(
    openAdd,
    !addOpen && stopIds == null && removeIds == null && tagIds == null,
  );

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
                  removeTitle: removeSelectedTitle(toRemove(selectedRows)),
                }
              : null
          }
          confirmRemove={confirmRemove}
          onConfirmRemove={updateConfirmRemove}
          updates={{
            prefs: updates.prefs,
            check: updates.check,
            onCheck: () => void updater.checkNow(),
            onAutoCheck: (on) => updater.setAutoCheck(on).catch(fail),
            onLeave: () => updater.clearCheckResult(),
          }}
          onAdd={() => openAdd()}
          prefs={prefs}
          resolved={resolved}
          onPrefs={updatePrefs}
          onSettingsApplied={applySettings}
          onSettingsUnsaved={notify}
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
        <div data-testid="update-bars" data-checks-done={updates.checksDone}>
          <UpdateNotice
            notice={updates.notice}
            willPause={willPauseCount(downloads)}
            staged={updates.staged}
            onRestart={() => void updater.install("now")}
            onInstallOnQuit={() => void updater.install("on-quit")}
            onLater={() => updater.later()}
            onNotes={() => api.openUpdateNotes().catch(fail)}
            onRetry={() => void updater.retry()}
            onDismiss={() => updater.dismiss()}
          />
          <PausedForUpdateBar
            count={pausedBarCount(downloads, updates)}
            heading={updatedHeading(updates.prefs)}
            onResumeAll={() => updater.resumePaused().catch(fail)}
            onDismiss={() => updater.forgetPaused().catch(fail)}
          />
        </div>
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
            work={laneWork.get(detailsRow.id) ?? EMPTY_WORK}
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
      {removeOpen && (
        <RemoveDialog ds={removeRows} onCancel={closeRemove} onConfirm={confirmRemoveRows} />
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
      {updates.prefs?.autoCheck === null && (
        <FirstRunCard onAnswer={(yes) => updater.answer(yes).catch(fail)} />
      )}
      {draggingUrl && <DropOverlay />}
    </div>
  );
}
