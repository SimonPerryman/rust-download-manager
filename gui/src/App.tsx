import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "./lib/api";
import {
  addedMessage,
  deletedPartialMessage,
  emptyCopy,
  openFailedMessage,
  removedMissingMessage,
  showNotFoundLabel,
  removeSelectedTitle,
  retryAllMessage,
} from "./lib/copy";
import { retryBatchMessage, rollupText } from "./lib/batches";
import { finishedToArchive, toRemove } from "./lib/archive";
import { knownUrls } from "./lib/batch";
import { focusIsFree } from "./lib/focus";
import { EMPTY_WORK } from "./lib/laneWork";
import { selectionActions } from "./lib/menu";
import { queueOrder, stepIndex } from "./lib/queue";
import { pauseAllMode, totalSpeed } from "./lib/rowView";
import { inOldFolder, missingIds, reviewRows, searchingMany, searchingOne } from "./lib/find";
import { locateFolderFor, locateOne } from "./lib/locate";
import type { DownloadSnapshot, SameFolderOffer } from "./lib/types";
import { useArchive } from "./lib/useArchive";
import { useBulkCommands } from "./lib/useBulkCommands";
import { useDetails } from "./lib/useDetails";
import { useDownloads } from "./lib/useDownloads";
import { useFind } from "./lib/useFind";
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
import { useSchedule } from "./lib/useSchedule";
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
import { SchedulesView } from "./components/schedules/SchedulesView";
import { Sidebar } from "./components/sidebar/Sidebar";
import { StatusBar } from "./components/statusbar/StatusBar";
import type { RowHandlers } from "./components/table/rowHandlers";
import { DownloadTable } from "./components/table/DownloadTable";
import { RowMenu } from "./components/table/RowMenu";
import { Toolbar } from "./components/toolbar/Toolbar";
import { FirstRunCard } from "./components/updater/FirstRunCard";
import { FindFilesBar } from "./components/missing/FindFilesBar";
import { FindReviewPane } from "./components/missing/FindReviewPane";
import { SameFolderOfferBar } from "./components/missing/SameFolderOfferBar";
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
    batches,
    items,
    toggleBatch,
    visible,
    failed,
    title,
  } = useView(downloads, prefs.groupBatches);

  const [page, setPage] = useState<"downloads" | "schedules">("downloads");
  const { schedule, queue: queueState, save: saveSchedule } = useSchedule();
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
    selection: sel,
    visibleIds,
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

  const find = useFind(downloads, notify, fail);
  const review = useMemo(
    () => (find.state.review ? reviewRows(find.state.review, downloads, find.state.results) : null),
    [find.state.review, find.state.results, downloads],
  );
  // Opening another row's details leaves the review.
  const { closeReview } = find;
  useEffect(() => closeReview(), [detailsId, closeReview]);
  const [sameFolderOffer, setSameFolderOffer] = useState<SameFolderOffer | null>(null);
  // Why the last Locate… was refused (a file of a different size), shown where it was asked for.
  const [locateError, setLocateError] = useState<{ id: number; message: string } | null>(null);
  const detailsIdRef = useRef<number | null>(null);
  detailsIdRef.current = detailsId;
  const locateDownload = useCallback(
    (d: DownloadSnapshot) => {
      setLocateError(null);
      locateOne(d)
        .then(({ message, offer }) => {
          if (message) notify(message);
          setSameFolderOffer(offer);
        })
        .catch((e) => {
          // The pane (or the Find panels) say it beside Locate…; with no pane open, a toast does.
          if (detailsIdRef.current === d.id) setLocateError({ id: d.id, message: String(e) });
          else fail(e);
        });
    },
    [notify, fail],
  );
  const dismissSameFolderOffer = useCallback(() => {
    setSameFolderOffer(null);
    api.dismissSameFolderOffer().catch(fail);
  }, [fail]);
  // Search looks for the files that were in the same old folder; the results are reviewed, so
  // nothing is relinked until the user says.
  const searchSameFolder = useCallback(() => {
    if (!sameFolderOffer) return;
    const ids = inOldFolder(downloads, sameFolderOffer.old_folder);
    dismissSameFolderOffer();
    find.startMany(ids.length ? ids : undefined);
  }, [sameFolderOffer, downloads, dismissSameFolderOffer, find.startMany]);
  const { select: selectIds } = selection;
  const locateFolder = useCallback(
    (rows: DownloadSnapshot[]) => {
      locateFolderFor(rows)
        .then((result) => {
          if (!result) return;
          const { message, notFoundIds } = result;
          const showNotFound = () => {
            setQuery("");
            setFilter({ kind: "status", status: "missing" });
            selectIds(notFoundIds);
          };
          notify(
            message,
            notFoundIds.length ? { label: showNotFoundLabel, run: showNotFound } : undefined,
          );
        })
        .catch(fail);
    },
    [notify, fail, setQuery, setFilter, selectIds],
  );
  // Find file shows its answer in the details pane, so the pane opens on the record.
  const { start: startFind } = find;
  const findDownload = useCallback(
    (d: DownloadSnapshot) => {
      setLocateError(null);
      setPaneTab("overview");
      setDetailsId(d.id);
      startFind(d.id);
    },
    [startFind, setDetailsId],
  );
  // Only the record: the file is already gone, so there is nothing to ask about.
  const forgetMissing = useCallback(
    (d: DownloadSnapshot) => {
      api
        .remove(d.id)
        .then(() => notify(removedMissingMessage(d)))
        .catch(fail);
    },
    [notify, fail],
  );

  const retryAll = useCallback(() => {
    const message = retryAllMessage(failed);
    api
      .retryFailed()
      .then(() => notify(message))
      .catch(fail);
  }, [failed, notify, fail]);

  const retryBatch = useCallback(
    (batch: number) => {
      api
        .retryBatch(batch)
        .then((n) => notify(retryBatchMessage(n)))
        .catch(fail);
    },
    [notify, fail],
  );

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
      locate: locateDownload,
      find: findDownload,
      forget: forgetMissing,
      remove: (d) => askRemove([d]),
      archive: (d) => archiver.archive([d]),
      deletePartial: (d) =>
        api
          .stop(d.id, false)
          .then(() => notify(deletedPartialMessage(d)))
          .catch(fail),
      retryAll,
    }),
    [
      fail,
      notify,
      openDownload,
      revealDownload,
      locateDownload,
      findDownload,
      forgetMissing,
      retryAll,
      askRemove,
      archiver.archive,
    ],
  );

  const run = useBulkCommands({
    downloads,
    queue,
    notify,
    fail,
    open: openDownload,
    reveal: revealDownload,
    locate: locateDownload,
    find: findDownload,
    locateFolder,
    forget: forgetMissing,
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
        onFilter={(f) => {
          setPage("downloads");
          setFilter(f);
        }}
        counts={counts}
        folders={folders}
        archivedCount={archived}
        batches={batches}
        tags={tags}
        bumpSeq={finish.bumpSeq}
        speed={totalSpeed(downloads)}
        speedSamples={history.total}
        globalLimit={settings?.global_limit ?? null}
        schedulesOpen={page === "schedules"}
        onSchedules={() => setPage("schedules")}
      />
      <main className={page === "schedules" ? "flex min-h-0 min-w-0 flex-col" : "main"}>
        {page === "schedules" ? (
          <SchedulesView
            schedule={schedule}
            queue={queueState}
            save={saveSchedule}
            onError={fail}
          />
        ) : (
          <>
            <Toolbar
              title={title}
              count={visible.length}
              batch={(() => {
                const group =
                  view.kind === "batch" ? batches.find((b) => b.id === view.batch) : null;
                return group
                  ? {
                      rollup: rollupText(group),
                      onRetry: group.failed > 0 ? () => retryBatch(group.id) : null,
                    }
                  : null;
              })()}
              hasBatches={batches.length > 0}
              onRetryAll={
                view.kind === "status" && view.status === "failed" && failed.length > 0
                  ? retryAll
                  : null
              }
              onFindFiles={
                view.kind === "status" &&
                view.status === "missing" &&
                visible.length > 0 &&
                !find.state.active
                  ? () => find.startMany(missingIds(downloads))
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
                  ? {
                      count: selectedRows.length,
                      actions: selectionActions(selectedRows, inArchive),
                    }
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
              <FindFilesBar search={searchingMany(find.state)} onCancel={find.cancel} />
              <SameFolderOfferBar
                offer={sameFolderOffer}
                onSearch={searchSameFolder}
                onDismiss={dismissSameFolderOffer}
              />
              <PausedForUpdateBar
                count={pausedBarCount(downloads, updates)}
                heading={updatedHeading(updates.prefs)}
                onResumeAll={() => updater.resumePaused().catch(fail)}
                onDismiss={() => updater.forgetPaused().catch(fail)}
              />
            </div>
            <DownloadTable
              items={items}
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
              onToggleBatch={toggleBatch}
              onRetryBatch={retryBatch}
              onRevealBatch={(batch) => api.revealBatch(batch).catch(fail)}
            />
            {review && (
              <FindReviewPane
                // A new search is a new review; its ticks start over.
                key={review.map((r) => r.d.id).join(",")}
                rows={review}
                height={pane.height}
                onRelink={(choices) => void find.relink(choices)}
                onLocate={locateDownload}
                onClose={find.closeReview}
              />
            )}
            {detailsRow && !review && (
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
                onLocate={locateDownload}
                onForget={forgetMissing}
                find={{
                  searching: searchingOne(find.state, detailsRow.id),
                  result: find.state.results[detailsRow.id],
                  locateError: locateError?.id === detailsRow.id ? locateError.message : null,
                  onFind: findDownload,
                  onCancel: find.cancel,
                  onRelink: (d, file) => void find.relink([{ id: d.id, path: file.path }]),
                  onSkip: (d) => find.skip(d.id),
                }}
                spot={
                  detailsRow.status === "queued" && queue.includes(detailsRow.id)
                    ? { index: queue.indexOf(detailsRow.id), count: queue.length }
                    : null
                }
                onMove={(delta) =>
                  moveInQueue(detailsRow.id, stepIndex(queue, detailsRow.id, delta))
                }
              />
            )}
          </>
        )}
      </main>
      <StatusBar
        speed={totalSpeed(downloads)}
        downloading={counts.downloading}
        waiting={counts.waiting}
        queue={queueState}
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
