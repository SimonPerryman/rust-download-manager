import { useEffect, useRef, useState } from "react";
import { NOTIFY_MS } from "../../lib/finish";
import { formatBytes } from "../../lib/format";
import type { DownloadSnapshot } from "../../lib/types";
import type { FinishNotice } from "../../lib/useFinishMoment";
import { Check } from "../icons/Check";
import { DownloadMark } from "../icons/DownloadMark";
import { ExternalLink } from "../icons/ExternalLink";
import { Folder } from "../icons/Folder";
import { X } from "../icons/X";
import { useEntered } from "./useEntered";

const SLIDE_MS = 350;

type Props = {
  notifications: FinishNotice[];
  onOpen: (d: DownloadSnapshot) => void;
  onReveal: (d: DownloadSnapshot) => void;
  onDismiss: (id: number) => void;
};

// The in-app "Download finished" cards, bottom right, newest last. Each goes
// after `NOTIFY_MS` unless the pointer or focus is on it.
export function FinishNotifications({ notifications, onOpen, onReveal, onDismiss }: Props) {
  return (
    <div
      aria-live="polite"
      className="fixed bottom-12 right-4 z-65 grid w-[min(352px,calc(100vw-32px))] gap-2"
    >
      {notifications.map((n) => (
        <Card
          key={n.id}
          d={n.download}
          onOpen={onOpen}
          onReveal={onReveal}
          onDismiss={() => onDismiss(n.id)}
        />
      ))}
    </div>
  );
}

function Card({
  d,
  onOpen,
  onReveal,
  onDismiss,
}: {
  d: DownloadSnapshot;
  onOpen: (d: DownloadSnapshot) => void;
  onReveal: (d: DownloadSnapshot) => void;
  onDismiss: () => void;
}) {
  const [entered, onAnimationEnd] = useEntered(SLIDE_MS);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const dismiss = useRef(onDismiss);
  dismiss.current = onDismiss;

  // Restarts the full wait each time the pointer and focus have both left.
  useEffect(() => {
    if (hovered || focused) return;
    const t = setTimeout(() => dismiss.current(), NOTIFY_MS);
    return () => clearTimeout(t);
  }, [hovered, focused]);

  return (
    <div
      role="status"
      data-testid="finish-notification"
      data-id={d.id}
      data-entered={entered ? "true" : undefined}
      onAnimationEnd={onAnimationEnd}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onFocus={() => setFocused(true)}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setFocused(false);
      }}
      className="notif"
    >
      <span className="grid h-8 w-8 place-items-center rounded-md bg-btn text-on-btn">
        <DownloadMark size={16} />
      </span>
      <div className="min-w-0">
        <p className="text-xs text-g400">rdm, now</p>
        <p className="flex items-center gap-1 text-sm font-semibold">
          <Check size={14} className="text-success" />
          Download finished
        </p>
        <p className="truncate text-g600" title={d.filename}>
          {d.filename}
        </p>
        <p className="text-xs tabular-nums text-g400">{formatBytes(d.total ?? d.downloaded)}</p>
        <div className="mt-2 flex gap-2">
          <button
            className="btn btn-primary btn-sm"
            onClick={() => {
              onOpen(d);
              onDismiss();
            }}
          >
            <ExternalLink size={14} />
            Open
          </button>
          <button
            className="btn btn-soft btn-sm"
            onClick={() => {
              onReveal(d);
              onDismiss();
            }}
          >
            <Folder size={14} />
            Show in folder
          </button>
        </div>
      </div>
      <button className="icon-btn icon-btn-sm" aria-label="Dismiss" onClick={onDismiss}>
        <X size={14} />
      </button>
    </div>
  );
}
