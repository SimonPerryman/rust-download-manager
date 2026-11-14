import { archiveTitle, removeTitle } from "../../lib/copy";
import { limitText } from "../../lib/rate";
import { rowActions, type RowAction, type RowActionKey } from "../../lib/rowView";
import type { DownloadSnapshot } from "../../lib/types";
import type { LimitEditor } from "../../lib/useLimitEditor";
import { Archive } from "../icons/Archive";
import { DownloadAgain } from "../icons/DownloadAgain";
import { ExternalLink } from "../icons/ExternalLink";
import { Folder } from "../icons/Folder";
import { Gauge } from "../icons/Gauge";
import { ListRemove } from "../icons/ListRemove";
import { Pause } from "../icons/Pause";
import { Play } from "../icons/Play";
import { Retry } from "../icons/Retry";
import { Search } from "../icons/Search";
import { Stop } from "../icons/Stop";
import { Trash } from "../icons/Trash";
import type { RowHandlers } from "./rowHandlers";
import { RowLimitEditor } from "./RowLimitEditor";

const ICON: Record<RowActionKey, (p: { size?: number }) => React.ReactNode> = {
  limit: Gauge,
  pause: Pause,
  resume: Play,
  stop: Stop,
  open: ExternalLink,
  locate: Search,
  reveal: Folder,
  remove: ListRemove,
  archive: Archive,
  retry: Retry,
  deletePartial: Trash,
  downloadAgain: DownloadAgain,
};

type Props = {
  d: DownloadSnapshot;
  lingering: boolean;
  editor: LimitEditor;
  handlers: RowHandlers;
};

// The row's buttons, always visible. The editor slot comes first and keeps its
// place across statuses, so a waiting row that starts keeps an open editor
// mounted with its focus and draft.
export function RowActions({ d, lingering, editor, handlers }: Props) {
  const run = (key: RowActionKey) => {
    switch (key) {
      case "limit":
        return editor.draft != null ? editor.commit(undefined, true) : editor.open();
      case "pause":
        return handlers.pause(d);
      case "resume":
      case "retry":
      case "downloadAgain":
        return handlers.resume(d);
      case "stop":
        return handlers.stop(d);
      case "open":
        return handlers.open(d);
      case "locate":
        return handlers.locate(d);
      case "reveal":
        return handlers.reveal(d);
      case "remove":
        return handlers.remove(d);
      case "archive":
        return handlers.archive(d);
      case "deletePartial":
        return handlers.deletePartial(d);
    }
  };
  const titleOf = (a: RowAction) => {
    if (a.key === "remove") return removeTitle(d);
    if (a.key === "archive") return archiveTitle;
    if (a.key === "limit" && d.limit != null) return `${a.label}: ${limitText(d.limit)}`;
    return a.label;
  };
  return (
    <div className="flex items-center justify-end gap-1">
      {editor.draft != null ? <RowLimitEditor editor={editor} /> : null}
      {rowActions(d, lingering).map((a) => (
        <ActionButton
          key={a.key}
          action={a}
          title={titleOf(a)}
          editor={a.key === "limit" ? editor : undefined}
          set={a.key === "limit" && d.limit != null}
          onClick={() => run(a.key)}
        />
      ))}
    </div>
  );
}

function ActionButton({
  action,
  title,
  editor,
  set = false,
  onClick,
}: {
  action: RowAction;
  title: string;
  /** Only for the gauge: it keeps focus off itself so an open editor isn't blurred. */
  editor?: LimitEditor;
  /** Tinted while what it changes is set (the gauge, while the row has a limit). */
  set?: boolean;
  onClick: () => void;
}) {
  const Icon = ICON[action.key];
  if (action.style === "icon") {
    return (
      <button
        ref={editor?.buttonRef}
        aria-label={action.label}
        title={title}
        aria-pressed={editor ? editor.draft != null : undefined}
        onMouseDown={editor ? (e) => e.preventDefault() : undefined}
        onClick={onClick}
        className={`icon-btn icon-btn-sm${set ? " is-set" : ""}`}
      >
        <Icon size={14} />
      </button>
    );
  }
  return (
    <button
      aria-label={action.label}
      title={title}
      onClick={onClick}
      className={`btn btn-sm ${action.style === "primary" ? "btn-primary" : "btn-soft"}`}
    >
      <Icon size={14} />
      <span className="act-label">{action.label}</span>
    </button>
  );
}
