import type { MenuCommand } from "../../lib/menu";
import type { Prefs } from "../../lib/prefs";
import type { SettingsField } from "../../lib/settings";
import type { Settings } from "../../lib/types";
import { Archive } from "../icons/Archive";
import { ArchiveRestore } from "../icons/ArchiveRestore";
import { ListRemove } from "../icons/ListRemove";
import { Pause } from "../icons/Pause";
import { Play } from "../icons/Play";
import { Plus } from "../icons/Plus";
import { Retry } from "../icons/Retry";
import { SettingsMenu } from "../settings/SettingsMenu";
import type { UpdateSettingsProps } from "../settings/UpdateSettings";
import { DensityToggle } from "./DensityToggle";
import { NameFilter } from "./NameFilter";
import { SelectionBar } from "./SelectionBar";
import { ThemeToggle } from "./ThemeToggle";

type Props = {
  title: string;
  /** Rows the view shows. */
  count: number;
  /** The Failed view's bulk retry; null hides it. */
  onRetryAll: (() => void) | null;
  query: string;
  onQuery: (q: string) => void;
  pauseMode: "pause" | "resume" | null;
  onPauseAll: () => void;
  onResumeAll: () => void;
  /** Archive finished; null hides it (the Archived view). */
  onArchiveFinished: (() => void) | null;
  canArchiveFinished: boolean;
  /** The Archived view's actions on the selected row(s); null elsewhere. */
  archived: {
    canAct: boolean;
    onUnarchive: () => void;
    onRemove: () => void;
    /** Remove's tooltip: what it does on disk to the selection. */
    removeTitle: string;
  } | null;
  onAdd: () => void;
  prefs: Prefs;
  resolved: "light" | "dark";
  onPrefs: (patch: Partial<Prefs>) => void;
  onSettingsApplied: (applied: Settings, fields: SettingsField[]) => void;
  /** Why a setting edit was refused after the popover closed (a toast). */
  onSettingsUnsaved: (message: string) => void;
  /** Whether Remove asks first (a Settings toggle). */
  confirmRemove: boolean;
  onConfirmRemove: (ask: boolean) => void;
  /** Settings' Updates section. */
  updates: UpdateSettingsProps;
  /** Two or more rows selected: the toolbar turns into their actions. */
  selection: { count: number; actions: MenuCommand[] } | null;
  onSelectionCommand: (cmd: MenuCommand) => void;
  onClearSelection: () => void;
};

// The view's heading and the window-wide actions; Add is the one solid primary.
// With several rows selected it shows what applies to them instead.
export function Toolbar(p: Props) {
  if (p.selection) {
    return (
      <header className="toolbar selecting flex min-w-0 items-center gap-3 pl-4 pr-3">
        <SelectionBar
          count={p.selection.count}
          actions={p.selection.actions}
          onRun={p.onSelectionCommand}
          onClear={p.onClearSelection}
        />
      </header>
    );
  }
  return (
    <header className="toolbar flex min-w-0 items-center gap-3 pl-4 pr-3">
      <div className="flex min-w-0 flex-1 items-center gap-3">
        <h1 className="truncate text-lg font-semibold tracking-tight">{p.title}</h1>
        <span data-testid="view-count" className="text-sm tabular-nums text-g400">
          {p.count}
        </span>
        {p.onRetryAll && (
          <button className="btn btn-soft btn-sm" onClick={p.onRetryAll}>
            <Retry size={14} />
            Retry all failed
          </button>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <NameFilter query={p.query} onQuery={p.onQuery} />
        {p.pauseMode === "pause" && (
          <button className="btn btn-ghost" aria-label="Pause all" onClick={p.onPauseAll}>
            <Pause />
            <span className="tb-label">Pause all</span>
          </button>
        )}
        {p.pauseMode === "resume" && (
          <button className="btn btn-ghost" aria-label="Resume all" onClick={p.onResumeAll}>
            <Play />
            <span className="tb-label">Resume all</span>
          </button>
        )}
        {p.onArchiveFinished && (
          <button
            className="btn btn-ghost"
            aria-label="Archive finished"
            disabled={!p.canArchiveFinished}
            onClick={p.onArchiveFinished}
            title="Move completed and stopped downloads to Archived, in every folder. Nothing is deleted."
          >
            <Archive />
            <span className="tb-label">Archive finished</span>
          </button>
        )}
        {p.archived && (
          <>
            <button
              className="btn btn-ghost"
              aria-label="Unarchive"
              disabled={!p.archived.canAct}
              onClick={p.archived.onUnarchive}
              title="Move the selected download back to its views"
            >
              <ArchiveRestore />
              <span className="tb-label">Unarchive</span>
            </button>
            <button
              className="btn btn-ghost"
              aria-label="Remove"
              disabled={!p.archived.canAct}
              onClick={p.archived.onRemove}
              title={p.archived.removeTitle}
            >
              <ListRemove />
              <span className="tb-label">Remove</span>
            </button>
          </>
        )}
        <button
          data-testid="add-button"
          className="btn btn-primary"
          onClick={p.onAdd}
          title="Add downloads, or paste links anywhere with Ctrl+V"
        >
          <Plus />
          Add
        </button>
        <span className="mx-1 h-5 w-px bg-line-strong" />
        <DensityToggle density={p.prefs.density} onPrefs={p.onPrefs} />
        <ThemeToggle resolved={p.resolved} onPrefs={p.onPrefs} />
        <SettingsMenu
          prefs={p.prefs}
          onPrefs={p.onPrefs}
          onApplied={p.onSettingsApplied}
          onUnsaved={p.onSettingsUnsaved}
          confirmRemove={p.confirmRemove}
          onConfirmRemove={p.onConfirmRemove}
          updates={p.updates}
        />
      </div>
    </header>
  );
}
