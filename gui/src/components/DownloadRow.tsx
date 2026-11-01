import type { DownloadSnapshot } from "../lib/types";
import {
  formatBytes,
  formatSpeed,
  formatDuration,
  formatAgo,
  percent,
  host,
} from "../lib/format";
import * as api from "../lib/api";
import {
  FileIcon,
  Pause,
  Play,
  X,
  Retry,
  Trash,
  Clock,
  Check,
  AlertTriangle,
  ExternalLink,
  Folder,
} from "./icons";

// Renders one download row, dispatching on `status`. Every control is a direct
// call to the typed api wrappers (which are thin wrappers over the engine's
// Manager); the row holds no download logic, only presentation.
export function DownloadRow({ d }: { d: DownloadSnapshot }) {
  switch (d.status) {
    case "downloading":
      return <DownloadingRow d={d} />;
    case "paused":
      return <PausedRow d={d} />;
    case "queued":
      return <QueuedRow d={d} />;
    case "failed":
      return <FailedRow d={d} />;
    case "completed":
      return <CompletedRow d={d} />;
    case "cancelled":
      return <CancelledRow d={d} />;
  }
}

// ---- shared pieces -------------------------------------------------------

function Shell({
  dim,
  children,
  actions,
}: {
  dim?: boolean;
  children: React.ReactNode;
  actions: React.ReactNode;
}) {
  return (
    <li className="flex items-start gap-4 px-5 py-4">
      <div
        className={`mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-muted ${
          dim ? "text-muted-foreground/60" : "text-muted-foreground"
        }`}
      >
        <FileIcon size={17} />
      </div>
      <div className="min-w-0 flex-1">{children}</div>
      <div className="flex shrink-0 items-center gap-0.5 self-center">
        {actions}
      </div>
    </li>
  );
}

function IconButton({
  title,
  onClick,
  children,
}: {
  title: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      title={title}
      onClick={onClick}
      className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
    >
      {children}
    </button>
  );
}

function Title({ name, dim }: { name: string; dim?: boolean }) {
  return (
    <p
      className={`truncate text-sm font-medium ${dim ? "text-muted-foreground" : ""}`}
    >
      {name}
    </p>
  );
}

function sizeLabel(d: DownloadSnapshot): string {
  return d.total != null ? formatBytes(d.total) : formatBytes(d.downloaded);
}

function Source({ d, extra }: { d: DownloadSnapshot; extra?: string }) {
  return (
    <p className="mt-0.5 text-xs text-muted-foreground">
      {host(d.url)} · {sizeLabel(d)}
      {extra ? ` · ${extra}` : ""}
    </p>
  );
}

function ProgressBar({
  pct,
  tone,
}: {
  pct: number | null;
  tone: "brand" | "muted";
}) {
  const width = pct ?? 0;
  const bar = tone === "brand" ? "bg-brand" : "bg-muted-foreground/50";
  return (
    <div className="mt-2.5 flex items-center gap-3">
      <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
        <div className={`h-full rounded-full ${bar}`} style={{ width: `${width}%` }} />
      </div>
      <span
        className={`w-9 shrink-0 text-right text-xs font-medium tabular-nums ${
          tone === "muted" ? "text-muted-foreground" : ""
        }`}
      >
        {pct != null ? `${pct}%` : "—"}
      </span>
    </div>
  );
}

// ---- status rows ---------------------------------------------------------

function DownloadingRow({ d }: { d: DownloadSnapshot }) {
  const pct = percent(d);
  return (
    <Shell
      actions={
        <>
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
      <Source d={d} />
      <ProgressBar pct={pct} tone="brand" />
      <div className="mt-1.5 flex items-center gap-4 text-xs text-muted-foreground">
        {d.bytes_per_sec > 0 && (
          <span className="tabular-nums">{formatSpeed(d.bytes_per_sec)}</span>
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

function PausedRow({ d }: { d: DownloadSnapshot }) {
  const pct = percent(d);
  return (
    <Shell
      actions={
        <>
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
      <Source d={d} />
      <ProgressBar pct={pct} tone="muted" />
      <div className="mt-1.5 flex items-center gap-4 text-xs text-muted-foreground">
        <span className="italic">resumes where it left off</span>
        <span className="tabular-nums">
          {formatBytes(d.downloaded)}
          {d.total != null ? ` / ${formatBytes(d.total)}` : ""}
        </span>
      </div>
    </Shell>
  );
}

function QueuedRow({ d }: { d: DownloadSnapshot }) {
  return (
    <Shell
      dim
      actions={
        <>
          <IconButton title="Start now" onClick={() => api.resume(d.id)}>
            <Play />
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
      <Source d={d} />
      <p className="mt-2 text-xs text-muted-foreground">
        Waiting — starts shortly.
      </p>
    </Shell>
  );
}

function FailedRow({ d }: { d: DownloadSnapshot }) {
  // The engine has no "retrying" status; a failed download is retryable via
  // resume. Show the error and a Retry action (mockup's "Retrying"/"Failed").
  return (
    <Shell
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
      <div className="mt-2 flex items-center gap-1.5 text-xs text-red-600/90 dark:text-red-400/90">
        <AlertTriangle size={13} />
        {d.error ?? "download failed"}
      </div>
    </Shell>
  );
}

function CompletedRow({ d }: { d: DownloadSnapshot }) {
  const path = d.final_path ?? d.target;
  return (
    <Shell
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
      <Source d={d} extra={d.finished_at != null ? `finished ${formatAgo(d.finished_at)}` : undefined} />
    </Shell>
  );
}

function CancelledRow({ d }: { d: DownloadSnapshot }) {
  return (
    <Shell
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
      <Source d={d} extra={d.finished_at != null ? `cancelled ${formatAgo(d.finished_at)}` : undefined} />
    </Shell>
  );
}
