import {
  NOTICE_ACTION_LABELS,
  noticeView,
  progressFraction,
  progressText,
  type Notice,
  type NoticeAction,
  type NoticeView,
} from "../../lib/updater";
import { AlertTriangle } from "../icons/AlertTriangle";
import { ArrowUp } from "../icons/ArrowUp";
import { Check } from "../icons/Check";

type Props = {
  notice: Notice;
  /** Downloads Restart now would pause. */
  willPause: number;
  /** The version staged to install at quit, if any. */
  staged: string | null;
  onRestart: () => void;
  onInstallOnQuit: () => void;
  onLater: () => void;
  /** Open the release page (What's new, and Download where rdm can't install itself). */
  onNotes: () => void;
  onRetry: () => void;
  onDismiss: () => void;
};

const TONE: Record<NoticeView["tone"], { bar: string; icon: string }> = {
  info: { bar: "bg-primary-soft", icon: "text-primary-ink" },
  done: { bar: "bg-success-soft", icon: "text-success-ink" },
  error: { bar: "bg-danger-soft", icon: "text-danger-ink" },
};

const ACTION_CLASS: Record<NoticeAction, string> = {
  later: "text-link",
  dismiss: "text-link",
  "install-on-quit": "btn btn-line h-7",
  "restart-instead": "btn btn-line h-7",
  retry: "btn btn-line h-7",
  restart: "btn btn-primary h-7",
  download: "btn btn-primary h-7",
};

// The slim "Update available" bar under the toolbar, through downloading,
// staged for the quit, restarting and failed. Restart now is always the
// primary action, Install when I quit the quieter one, Later a text link.
export function UpdateNotice(p: Props) {
  const view = noticeView(p.notice, { willPause: p.willPause, staged: p.staged });
  if (!view) return null;
  const run: Record<NoticeAction, () => void> = {
    later: p.onLater,
    "install-on-quit": p.onInstallOnQuit,
    restart: p.onRestart,
    "restart-instead": p.onRestart,
    download: p.onNotes,
    dismiss: p.onDismiss,
    retry: p.onRetry,
  };
  const Icon = view.tone === "done" ? Check : view.tone === "error" ? AlertTriangle : ArrowUp;
  const fraction = progressFraction(view.progress);

  return (
    <div
      data-testid="update-notice"
      data-state={p.notice.kind}
      className={`flex min-w-0 items-center gap-3 border-b border-line px-4 py-2 text-sm text-ink ${TONE[view.tone].bar}`}
    >
      <Icon size={16} className={`shrink-0 ${TONE[view.tone].icon}`} />
      <span className="flex min-w-0 flex-col">
        <span>
          {/* The live region is the title alone: progress and buttons would chatter. */}
          <strong role="status" className="font-semibold">
            {view.title}
          </strong>
          {view.notes && (
            <>
              {" · "}
              <button className="btn-link underline" onClick={p.onNotes}>
                What's new
              </button>
            </>
          )}
          {view.detail && (
            <>
              {" · "}
              <span data-testid="update-detail" className="text-g600">
                {view.detail}
              </span>
            </>
          )}
        </span>
        {view.lines.map((line) => (
          <span key={line} data-testid="update-line" className="text-g600">
            {line}
          </span>
        ))}
      </span>
      {view.downloading && (
        <>
          <span
            role="progressbar"
            aria-label="Update download"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={fraction === null ? undefined : Math.round(fraction * 100)}
            className="relative h-1 w-36 shrink-0 overflow-hidden rounded-full bg-track"
          >
            <span
              className="absolute inset-y-0 left-0 bg-fill"
              style={{ width: `${(fraction ?? 0) * 100}%` }}
            />
          </span>
          <span data-testid="update-progress" className="shrink-0 tabular-nums text-g600">
            {progressText(view.progress)}
          </span>
        </>
      )}
      {view.actions.length > 0 && (
        <span className="ml-auto flex shrink-0 items-center gap-2">
          {view.actions.map((a) => (
            <button key={a} className={ACTION_CLASS[a]} onClick={run[a]}>
              {NOTICE_ACTION_LABELS[a]}
            </button>
          ))}
        </span>
      )}
    </div>
  );
}
