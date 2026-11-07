import { removeTitle } from "../../lib/copy";
import { rowActions, type RowAction, type RowActionKey } from "../../lib/rowView";
import type { DownloadSnapshot } from "../../lib/types";
import type { LimitEditor } from "../../lib/useLimitEditor";
import { DownloadAgain } from "../icons/DownloadAgain";
import { ExternalLink } from "../icons/ExternalLink";
import { Folder } from "../icons/Folder";
import { Gauge } from "../icons/Gauge";
import { ListRemove } from "../icons/ListRemove";
import { Pause } from "../icons/Pause";
import { Play } from "../icons/Play";
import { Retry } from "../icons/Retry";
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
  reveal: Folder,
  remove: ListRemove,
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
      case "reveal":
        return handlers.reveal(d);
      case "remove":
        return handlers.remove(d);
      case "deletePartial":
        return handlers.deletePartial(d);
    }
  };
  return (
    <div className="flex items-center justify-end gap-1">
      {editor.draft != null ? <RowLimitEditor editor={editor} /> : null}
      {rowActions(d, lingering).map((a) => (
        <ActionButton
          key={a.key}
          action={a}
          title={a.key === "remove" ? removeTitle(d) : a.label}
          editor={a.key === "limit" ? editor : undefined}
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
  onClick,
}: {
  action: RowAction;
  title: string;
  /** Only for the gauge: it keeps focus off itself so an open editor isn't blurred. */
  editor?: LimitEditor;
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
        className="icon-btn icon-btn-sm"
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
      {action.label}
    </button>
  );
}
