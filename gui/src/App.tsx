import { useEffect, useMemo, useState } from "react";
import * as api from "./lib/api";
import type { DownloadSnapshot } from "./lib/types";
import { TopBar } from "./components/TopBar";
import { Tabs, type Tab } from "./components/Tabs";
import { DownloadList } from "./components/DownloadList";
import { AddDialog } from "./components/AddDialog";

const ACTIVE = new Set(["queued", "downloading", "paused"]);

export default function App() {
  const [downloads, setDownloads] = useState<DownloadSnapshot[]>([]);
  const [tab, setTab] = useState<Tab>("active");
  const [dialogOpen, setDialogOpen] = useState(false);

  // Seed with a one-shot snapshot, then track the backend's live event stream.
  // All state flows from the engine; the UI never computes download state.
  useEffect(() => {
    api
      .list()
      .then(setDownloads)
      .catch(() => {});
    const unlisten = api.onDownloadsChanged(setDownloads);
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const active = useMemo(() => downloads.filter((d) => ACTIVE.has(d.status)), [downloads]);
  const history = useMemo(() => downloads.filter((d) => !ACTIVE.has(d.status)), [downloads]);
  const canPauseAll = active.some((d) => d.status === "downloading");

  return (
    <div className="mx-auto flex min-h-screen max-w-5xl flex-col">
      <TopBar
        onAdd={() => setDialogOpen(true)}
        onPauseAll={() => api.pauseAll()}
        canPauseAll={canPauseAll}
      />

      <main className="flex-1 px-5 py-6">
        <div className="mb-5 flex items-end justify-between">
          <div>
            <h1 className="text-xl font-semibold tracking-tight">Downloads</h1>
            <p className="mt-1 text-sm text-muted-foreground">
              Restored from <span className="font-mono text-[12.5px]">~/.rdm/state.json</span> —
              your list and history persist across restarts.
            </p>
          </div>
        </div>

        <Tabs
          tab={tab}
          onChange={setTab}
          activeCount={active.length}
          historyCount={history.length}
        />

        {tab === "active" ? (
          <DownloadList downloads={active} empty="No active downloads. Add one to get started." />
        ) : (
          <DownloadList
            downloads={history}
            empty="No history yet."
            footer={
              <p className="mt-3 px-1 text-xs text-muted-foreground">
                History is kept in <span className="font-mono text-[12px]">~/.rdm/state.json</span>{" "}
                and survives restarts. Removing an entry only clears it from this list.
              </p>
            }
          />
        )}
      </main>

      <AddDialog open={dialogOpen} onClose={() => setDialogOpen(false)} />
    </div>
  );
}
