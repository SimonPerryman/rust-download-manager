import { useEffect, useRef, useState } from "react";
import type { DownloadSnapshot } from "../lib/types";
import {
  formatBytes,
  formatSpeed,
  formatDuration,
  formatAgo,
  percent,
  host,
  connectionsLabel,
} from "../lib/format";
import * as api from "../lib/api";
import { NO_LIMIT, composeRate, exceeds, rateDraft, sameRate, type RateDraft } from "../lib/rate";
import { RateInput } from "./RateInput";
import { FileIcon } from "./icons/FileIcon";
import { Gauge } from "./icons/Gauge";
import { Pause } from "./icons/Pause";
import { Play } from "./icons/Play";
import { X } from "./icons/X";
import { Retry } from "./icons/Retry";
import { Trash } from "./icons/Trash";
import { Clock } from "./icons/Clock";
import { Check } from "./icons/Check";
import { AlertTriangle } from "./icons/AlertTriangle";
import { ExternalLink } from "./icons/ExternalLink";
import { Folder } from "./icons/Folder";

// Renders one download row, dispatching on `status`. Every control is a direct
// call to the typed api wrappers (which are thin wrappers over the engine's
// Manager); the row holds no download logic, only presentation.
export function DownloadRow({ d }: { d: DownloadSnapshot }) {
  const limit = useLimitEditor(d);
  // Plain render functions, not components: every status returns the same
  // <Shell> root with the meta line in the same slot, so an open limit editor
  // stays mounted (keeping focus and draft) when e.g. a queued row starts.
  switch (d.status) {
    case "downloading":
      return downloadingRow(d, limit);
    case "paused":
      return pausedRow(d, limit);
    case "queued":
      return queuedRow(d, limit);
    case "failed":
      return failedRow(d);
    case "completed":
      return completedRow(d);
    case "cancelled":
      return cancelledRow(d);
  }
}

// ---- speed limit ---------------------------------------------------------

// A speed cap only matters while a download can still stream, so finished
// rows neither show nor edit it.
const LIMIT_SHOWN: DownloadSnapshot["status"][] = ["downloading", "paused", "queued"];

/** The row's inline speed-limit editor; `draft` is null while it's closed. */
type LimitEditor = {
  draft: RateDraft | null;
  error?: string;
  /** The global cap, read when the editor opens, for the "won't go faster" note. */
  globalLimit: number | null;
  open: () => void;
  change: (d: RateDraft) => void;
  /** `refocus` returns focus to the gauge button (keyboard commits), not on a click-away. */
  commit: (next?: RateDraft, refocus?: boolean) => void;
  cancel: () => void;
  /** The gauge button, where focus goes back when the editor closes from the keyboard. */
  buttonRef: React.RefObject<HTMLButtonElement>;
};

// Commits on Enter or when focus leaves the editor; the engine validates and
// the row redraws from its snapshot. An edit still open when the row goes
// away is flushed, best effort.
function useLimitEditor(d: DownloadSnapshot): LimitEditor {
  const [draft, setDraft] = useState<RateDraft | null>(null);
  const [error, setError] = useState<string>();
  const [globalLimit, setGlobalLimit] = useState<number | null>(null);
  const pending = useRef(false);
  // A commit asked for while one was in flight (e.g. a click-away), replayed after it.
  const deferred = useRef<{ refocus: boolean } | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const latest = useRef({ draft, d });
  latest.current = { draft, d };

  const editable = LIMIT_SHOWN.includes(d.status);
  useEffect(() => {
    if (!editable) {
      setDraft(null);
      setError(undefined);
    }
  }, [editable]);

  useEffect(
    () => () => {
      const { draft, d } = latest.current;
      if (!draft || !LIMIT_SHOWN.includes(d.status) || sameRate(draft, rateDraft(d.limit))) return;
      const r = composeRate(draft);
      if (r.ok) api.setLimit(d.id, r.rate || null).catch(() => {});
    },
    [],
  );

  const close = (refocus = false) => {
    setDraft(null);
    setError(undefined);
    // After the editor unmounts, so focus isn't dropped on <body> — but never
    // taken back from somewhere the user has moved it in the meantime.
    if (refocus) {
      setTimeout(() => {
        const active = document.activeElement;
        if (!active || active === document.body) buttonRef.current?.focus();
      });
    }
  };

  const commit = async (next?: RateDraft, refocus = false) => {
    // `latest` rather than this render's `draft`: a blur fired as the editor
    // closes must not re-send a draft that was just committed or cancelled.
    const { draft, d } = latest.current;
    if (!draft) return;
    if (pending.current) {
      deferred.current = { refocus };
      return;
    }
    const value = next ?? draft;
    if (sameRate(value, rateDraft(d.limit))) return close(refocus);
    const r = composeRate(value);
    if (!r.ok) return setError(r.error);
    pending.current = true;
    try {
      await api.setLimit(d.id, r.rate || null);
      // Anything typed while this was in flight stays open for its own commit.
      if (sameRate(latest.current.draft ?? NO_LIMIT, value)) close(refocus);
    } catch (e) {
      setError(String(e));
    } finally {
      pending.current = false;
    }
    const again = deferred.current;
    deferred.current = null;
    // Replays only a draft newer than the one just sent (a no-op once closed).
    if (again && !sameRate(latest.current.draft ?? NO_LIMIT, value)) {
      await commit(undefined, again.refocus);
    }
  };

  return {
    draft,
    error,
    globalLimit,
    open: () => {
      setDraft(rateDraft(d.limit));
      setError(undefined);
      api
        .getSettings()
        .then((s) => setGlobalLimit(s.global_limit))
        .catch(() => setGlobalLimit(null));
    },
    change: (next) => {
      setDraft(next);
      setError(undefined);
    },
    commit,
    cancel: () => close(true),
    buttonRef,
  };
}

/** Opens the limit editor, or commits it if open. */
function LimitButton({ limit }: { limit: LimitEditor }) {
  const editing = limit.draft != null;
  return (
    <IconButton
      title="Speed limit"
      pressed={editing}
      keepFocus
      buttonRef={limit.buttonRef}
      onClick={() => (editing ? limit.commit(undefined, true) : limit.open())}
    >
      <Gauge />
    </IconButton>
  );
}

// ---- shared pieces -------------------------------------------------------

function Shell({
  d,
  dim,
  children,
  actions,
}: {
  d: DownloadSnapshot;
  dim?: boolean;
  children: React.ReactNode;
  actions: React.ReactNode;
}) {
  return (
    <li
      data-testid="download-row"
      data-id={d.id}
      data-filename={d.filename}
      data-status={d.status}
      data-limit={d.limit ?? ""}
      className="flex items-start gap-row-gap px-row-px py-row-py"
    >
      <div
        className={`mt-0.5 flex h-row-icon w-row-icon shrink-0 items-center justify-center rounded-md bg-muted ${
          dim ? "text-muted-foreground/60" : "text-muted-foreground"
        }`}
      >
        <FileIcon size={17} className="h-row-glyph w-row-glyph" />
      </div>
      <div className="min-w-0 flex-1">{children}</div>
      <div className="flex shrink-0 items-center gap-0.5 self-center">{actions}</div>
    </li>
  );
}

function IconButton({
  title,
  onClick,
  pressed,
  keepFocus,
  buttonRef,
  children,
}: {
  title: string;
  onClick: () => void;
  pressed?: boolean;
  buttonRef?: React.Ref<HTMLButtonElement>;
  /** Don't take focus on click, so an open editor isn't blurred by it. */
  keepFocus?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      ref={buttonRef}
      title={title}
      aria-label={title}
      aria-pressed={pressed}
      onMouseDown={keepFocus ? (e) => e.preventDefault() : undefined}
      onClick={onClick}
      className="inline-flex h-row-action w-row-action items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground aria-pressed:bg-accent aria-pressed:text-foreground"
    >
      {children}
    </button>
  );
}

function Title({ name, dim }: { name: string; dim?: boolean }) {
  return (
    <p className={`truncate text-row-title font-medium ${dim ? "text-muted-foreground" : ""}`}>
      {name}
    </p>
  );
}

function sizeLabel(d: DownloadSnapshot): string {
  return d.total != null ? formatBytes(d.total) : formatBytes(d.downloaded);
}

// The meta line. Given `limit` (rows that take one), a set cap shows as a
// clickable "limit 500.0 KB/s" and no cap shows nothing (the gauge action adds
// one); while editing, the cap becomes the inline editor.
function Source({ d, extra, limit }: { d: DownloadSnapshot; extra?: string; limit?: LimitEditor }) {
  const draft = limit?.draft;
  let message: React.ReactNode = null;
  if (limit && draft) {
    if (limit.error) {
      message = <span className="text-red-600 dark:text-red-400">{limit.error}</span>;
    } else if (limit.globalLimit != null && exceeds(draft, limit.globalLimit)) {
      message = `The global limit is ${formatSpeed(limit.globalLimit)} for all downloads combined, so this one won't go faster than that.`;
    } else if (draft.amount === "") {
      message = "Empty = no limit.";
    }
  }
  return (
    <div className="mt-0.5 text-row-meta text-muted-foreground">
      <p>
        {host(d.url)} · {sizeLabel(d)}
        {extra ? ` · ${extra}` : ""}
        {limit && draft && (
          <>
            {" · "}
            <span className="inline-block align-middle">
              <RateInput
                label="Speed limit"
                size="sm"
                autoFocus
                value={draft}
                onChange={limit.change}
                invalid={!!limit.error}
                onEnter={() => limit.commit(undefined, true)}
                onLeave={() => limit.commit()}
                onEscape={limit.cancel}
                onClear={() => {
                  limit.change(NO_LIMIT);
                  limit.commit(NO_LIMIT, true);
                }}
              />
            </span>
          </>
        )}
        {limit && !draft && d.limit != null && (
          <>
            {" · "}
            <button
              data-testid="row-limit"
              title="Change speed limit"
              onClick={limit.open}
              className="rounded border border-dashed border-transparent px-1 tabular-nums hover:border-input hover:text-foreground"
            >
              limit {formatSpeed(d.limit)}
            </button>
          </>
        )}
      </p>
      {message && <p className="mt-1 text-xs">{message}</p>}
    </div>
  );
}

function ProgressBar({ pct, tone }: { pct: number | null; tone: "brand" | "muted" }) {
  const width = pct ?? 0;
  const bar = tone === "brand" ? "bg-brand" : "bg-muted-foreground/50";
  return (
    <div className="mt-row-stack flex items-center gap-3">
      <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
        <div className={`h-full rounded-full ${bar}`} style={{ width: `${width}%` }} />
      </div>
      <span
        className={`w-9 shrink-0 text-right text-row-meta font-medium tabular-nums ${
          tone === "muted" ? "text-muted-foreground" : ""
        }`}
      >
        {pct != null ? `${pct}%` : "—"}
      </span>
    </div>
  );
}

// ---- status rows ---------------------------------------------------------

function downloadingRow(d: DownloadSnapshot, limit: LimitEditor) {
  const pct = percent(d);
  const connections = connectionsLabel(d);
  return (
    <Shell
      d={d}
      actions={
        <>
          <LimitButton limit={limit} />
          <IconButton title="Pause" onClick={() => api.pause(d.id)}>
            <Pause />
          </IconButton>
          <IconButton title="Cancel" onClick={() => api.cancel(d.id)}>
            <X />
          </IconButton>
        </>
      }
    >
      <div className="flex items-center gap-2">
        <Title name={d.filename} />
        <span className="inline-flex items-center gap-1 rounded-full border border-transparent bg-brand/10 px-2 py-0.5 text-[11px] font-medium text-brand dark:bg-brand/15">
          <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-brand" />
          Downloading
        </span>
      </div>
      <Source d={d} limit={limit} />
      <ProgressBar pct={pct} tone="brand" />
      <div className="mt-row-stack-sm flex items-center gap-4 text-row-meta text-muted-foreground">
        {d.bytes_per_sec > 0 && (
          <span className="tabular-nums">{formatSpeed(d.bytes_per_sec)}</span>
        )}
        {connections && (
          <span data-testid="row-connections" className="tabular-nums">
            {connections}
          </span>
        )}
        {d.eta_secs != null && (
          <span className="tabular-nums">ETA {formatDuration(d.eta_secs)}</span>
        )}
        <span className="tabular-nums">
          {formatBytes(d.downloaded)}
          {d.total != null ? ` / ${formatBytes(d.total)}` : ""}
        </span>
      </div>
    </Shell>
  );
}

function pausedRow(d: DownloadSnapshot, limit: LimitEditor) {
  const pct = percent(d);
  return (
    <Shell
      d={d}
      actions={
        <>
          <LimitButton limit={limit} />
          <IconButton title="Resume" onClick={() => api.resume(d.id)}>
            <Play />
          </IconButton>
          <IconButton title="Cancel" onClick={() => api.cancel(d.id)}>
            <X />
          </IconButton>
        </>
      }
    >
      <div className="flex items-center gap-2">
        <Title name={d.filename} />
        <span className="inline-flex items-center gap-1 rounded-full border border-transparent bg-amber-500/10 px-2 py-0.5 text-[11px] font-medium text-amber-600 dark:bg-amber-400/10 dark:text-amber-400">
          Paused
        </span>
      </div>
      <Source d={d} limit={limit} />
      <ProgressBar pct={pct} tone="muted" />
      <div className="mt-row-stack-sm flex items-center gap-4 text-row-meta text-muted-foreground">
        <span className="italic">resumes where it left off</span>
        <span className="tabular-nums">
          {formatBytes(d.downloaded)}
          {d.total != null ? ` / ${formatBytes(d.total)}` : ""}
        </span>
      </div>
    </Shell>
  );
}

function queuedRow(d: DownloadSnapshot, limit: LimitEditor) {
  return (
    <Shell
      d={d}
      dim
      actions={
        <>
          <LimitButton limit={limit} />
          <IconButton title="Pause" onClick={() => api.pause(d.id)}>
            <Pause />
          </IconButton>
          <IconButton title="Cancel" onClick={() => api.cancel(d.id)}>
            <X />
          </IconButton>
        </>
      }
    >
      <div className="flex items-center gap-2">
        <Title name={d.filename} dim />
        <span className="inline-flex items-center gap-1 rounded-full border border-transparent bg-muted px-2 py-0.5 text-[11px] font-medium text-muted-foreground">
          <Clock size={11} />
          Queued
        </span>
      </div>
      <Source d={d} limit={limit} />
      <p className="mt-row-note text-row-meta text-muted-foreground">Waiting for a free slot</p>
    </Shell>
  );
}

function failedRow(d: DownloadSnapshot) {
  // The engine has no "retrying" status; a failed download is retryable via
  // resume. Show the error and a Retry action (mockup's "Retrying"/"Failed").
  return (
    <Shell
      d={d}
      actions={
        <>
          <IconButton title="Retry" onClick={() => api.resume(d.id)}>
            <Retry />
          </IconButton>
          <IconButton title="Remove" onClick={() => api.remove(d.id)}>
            <Trash />
          </IconButton>
        </>
      }
    >
      <div className="flex items-center gap-2">
        <Title name={d.filename} />
        <span className="inline-flex items-center gap-1 rounded-full border border-transparent bg-red-500/10 px-2 py-0.5 text-[11px] font-medium text-red-600 dark:bg-red-400/10 dark:text-red-400">
          <X size={11} />
          Failed
        </span>
      </div>
      <Source d={d} extra={d.finished_at != null ? formatAgo(d.finished_at) : undefined} />
      <div className="mt-row-note flex items-center gap-1.5 text-row-meta text-red-600/90 dark:text-red-400/90">
        <AlertTriangle size={13} />
        {d.error ?? "download failed"}
      </div>
    </Shell>
  );
}

function completedRow(d: DownloadSnapshot) {
  const path = d.final_path ?? d.target;
  return (
    <Shell
      d={d}
      actions={
        <>
          <IconButton title="Open file" onClick={() => api.openPath(path)}>
            <ExternalLink />
          </IconButton>
          <IconButton title="Open folder" onClick={() => api.revealPath(path)}>
            <Folder />
          </IconButton>
          <IconButton title="Remove" onClick={() => api.remove(d.id)}>
            <Trash />
          </IconButton>
        </>
      }
    >
      <div className="flex items-center gap-2">
        <Title name={d.filename} />
        <span className="inline-flex items-center gap-1 rounded-full border border-transparent bg-emerald-500/10 px-2 py-0.5 text-[11px] font-medium text-emerald-600 dark:bg-emerald-400/10 dark:text-emerald-400">
          <Check size={11} />
          Completed
        </span>
      </div>
      <Source
        d={d}
        extra={d.finished_at != null ? `finished ${formatAgo(d.finished_at)}` : undefined}
      />
    </Shell>
  );
}

function cancelledRow(d: DownloadSnapshot) {
  return (
    <Shell
      d={d}
      dim
      actions={
        <>
          <IconButton title="Restart" onClick={() => api.resume(d.id)}>
            <Retry />
          </IconButton>
          <IconButton title="Remove" onClick={() => api.remove(d.id)}>
            <Trash />
          </IconButton>
        </>
      }
    >
      <div className="flex items-center gap-2">
        <Title name={d.filename} dim />
        <span className="inline-flex items-center gap-1 rounded-full border border-transparent bg-muted px-2 py-0.5 text-[11px] font-medium text-muted-foreground">
          Cancelled
        </span>
      </div>
      <Source
        d={d}
        extra={d.finished_at != null ? `cancelled ${formatAgo(d.finished_at)}` : undefined}
      />
    </Shell>
  );
}
