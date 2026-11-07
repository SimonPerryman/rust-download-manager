import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "./lib/api";
import {
  deletedPartialMessage,
  emptyCopy,
  openFailedMessage,
  removedMessage,
  retryAllMessage,
  stoppedMessage,
  viewTitle,
} from "./lib/copy";
import { matchesFilter, matchesQuery, statusCounts, type ViewFilter } from "./lib/filters";
import { deriveFolders, folderOf } from "./lib/folders";
import { planBatch } from "./lib/pendingClear";
import { focusIsFree } from "./lib/focus";
import { isStoppable, pauseAllMode, totalSpeed } from "./lib/rowView";
import { arrangeRows, DEFAULT_SORT, isVolatile, nextSort, rankOf, type SortKey } from "./lib/sort";
import type { DownloadSnapshot } from "./lib/types";
import { useDownloads } from "./lib/useDownloads";
import { useFinishMoment } from "./lib/useFinishMoment";
import { usePendingClear } from "./lib/usePendingClear";
import { usePrefs } from "./lib/usePrefs";
import { useSettings } from "./lib/useSettings";
import { useToast } from "./lib/useToast";
import { useUrlIntake } from "./lib/useUrlIntake";
import { AddDialog } from "./components/dialogs/AddDialog";
import { StopDialog } from "./components/dialogs/StopDialog";
import { DropOverlay } from "./components/DropOverlay";
import { FinishNotifications } from "./components/feedback/FinishNotifications";
import { Toasts } from "./components/feedback/Toasts";
import { Sidebar } from "./components/sidebar/Sidebar";
import { StatusBar } from "./components/statusbar/StatusBar";
import type { RowHandlers } from "./components/table/rowHandlers";
import { DownloadTable } from "./components/table/DownloadTable";
import { Toolbar } from "./components/toolbar/Toolbar";

const ALL: ViewFilter = { kind: "status", status: "all" };

// How long a confirmed stop may take to land before focus gives up on the row.
const REFOCUS_MS = 2_000;

// State wiring only: every download fact comes from the engine's snapshots,
// and every action is a thin api call.
export default function App() {
  const { downloads, seeded } = useDownloads();
  const [prefs, updatePrefs, resolved] = usePrefs();
  const { settings, apply: applySettings } = useSettings();
  const { toast, show: notify, dismiss: dismissToast } = useToast();
  const finish = useFinishMoment(downloads, seeded);
  const clear = usePendingClear(downloads, notify);

  const [filter, setFilter] = useState<ViewFilter>(ALL);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState(DEFAULT_SORT);
  const [frozen, setFrozen] = useState<Map<number, number> | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [prefillUrl, setPrefillUrl] = useState("");
  const [stopTarget, setStopTarget] = useState<number | null>(null);
  const [refocusId, setRefocusId] = useState<number | null>(null);
  const tableRef = useRef<HTMLDivElement>(null);

  const openAdd = useCallback((url = "") => {
    setPrefillUrl(url);
    setAddOpen(true);
  }, []);
  const draggingUrl = useUrlIntake(openAdd, !addOpen && stopTarget == null);

  // Rows hidden by a pending clear are left out of every view and count.
  const shown = useMemo(
    () => downloads.filter((d) => !clear.hidden.has(d.id)),
    [downloads, clear.hidden],
  );
  const counts = useMemo(() => statusCounts(shown), [shown]);
  const folders = useMemo(() => deriveFolders(downloads, shown), [downloads, shown]);

  // A folder view falls back to All only once no download at all is saved there
  // (not while a pending clear merely hides them).
  const folderGone =
    filter.kind === "folder" && !downloads.some((d) => folderOf(d.target) === filter.dir);
  const view = folderGone ? ALL : filter;
  useEffect(() => {
    if (folderGone) setFilter(ALL);
  }, [folderGone]);

  const visible = useMemo(
    () =>
      arrangeRows(
        shown.filter((d) => matchesFilter(d, view) && matchesQuery(d, query)),
        { sort, frozen, pinFailed: view.kind === "status" && view.status === "all" },
      ),
    [shown, view, query, sort, frozen],
  );
  const failed = useMemo(() => shown.filter((d) => d.status === "failed"), [shown]);
  const title = viewTitle(view, folders);

  const onSort = (key: SortKey) => {
    const next = nextSort(sort, key);
    setSort(next);
    // Live values would reshuffle rows under the pointer, so freeze the order they had.
    setFrozen(isVolatile(next.key) ? rankOf(downloads, next) : null);
  };

  const fail = useCallback((e: unknown) => notify(String(e)), [notify]);
  const { markOpened } = finish;

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
      stop: (d) => setStopTarget(d.id),
      open: openDownload,
      reveal: revealDownload,
      remove: (d) =>
        api
          .remove(d.id)
          .then(() => notify(removedMessage(d)))
          .catch(fail),
      deletePartial: (d) =>
        api
          .stop(d.id, false)
          .then(() => notify(deletedPartialMessage(d)))
          .catch(fail),
      retryAll,
    }),
    [fail, notify, openDownload, revealDownload, retryAll],
  );

  // The dialog follows the live row and closes itself once the row can't be stopped.
  const stopRow = downloads.find((d) => d.id === stopTarget);
  const stopOpen = isStoppable(stopRow);
  useEffect(() => {
    if (stopTarget == null || stopOpen) return;
    const id = stopTarget;
    setStopTarget(null);
    // The row finished or failed under the dialog: give focus back to that row.
    setTimeout(() => {
      if (!focusIsFree()) return;
      const button = document.querySelector<HTMLButtonElement>(
        `[data-testid="download-row"][data-id="${id}"] .c-act button`,
      );
      (button ?? tableRef.current)?.focus();
    });
  }, [stopTarget, stopOpen]);

  const cancelStop = useCallback(() => {
    const id = stopTarget;
    setStopTarget(null);
    if (id == null) return;
    setTimeout(() =>
      document
        .querySelector<HTMLButtonElement>(
          `[data-testid="download-row"][data-id="${id}"] button[aria-label="Stop"]`,
        )
        ?.focus(),
    );
  }, [stopTarget]);

  const confirmStop = (keep: boolean) => {
    const d = stopRow;
    setStopTarget(null);
    if (!isStoppable(d)) return;
    api
      .stop(d.id, keep)
      .then(() => {
        notify(stoppedMessage(d, keep));
        setRefocusId(d.id);
      })
      .catch(fail);
  };

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

  return (
    <div className="app">
      <Sidebar
        filter={view}
        onFilter={setFilter}
        counts={counts}
        folders={folders}
        bumpSeq={finish.bumpSeq}
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
          canClear={planBatch(shown, 0) != null}
          onClear={() => clear.clearFinished(shown)}
          onAdd={() => openAdd()}
          prefs={prefs}
          resolved={resolved}
          onPrefs={updatePrefs}
          onSettingsApplied={applySettings}
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
          wrapRef={tableRef}
        />
      </main>
      <StatusBar
        speed={totalSpeed(downloads)}
        downloading={counts.downloading}
        waiting={counts.waiting}
        settings={settings}
        applySettings={applySettings}
      />

      {stopOpen && <StopDialog d={stopRow} onCancel={cancelStop} onConfirm={confirmStop} />}
      <AddDialog open={addOpen} initialUrl={prefillUrl} onClose={() => setAddOpen(false)} />
      <Toasts
        pending={clear.pending}
        onUndo={clear.undo}
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
