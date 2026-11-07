import { useState } from "react";
import type { Batch } from "../../lib/pendingClear";
import type { Toast } from "../../lib/useToast";
import { X } from "../icons/X";
import { useEntered } from "./useEntered";

const RISE_MS = 200;

type Props = {
  /** The Clear finished batch in its undo window. */
  pending: Batch | null;
  onUndo: () => void;
  toast: Toast | null;
  onDismissToast: () => void;
};

// Bottom-centre toasts: the undo slot sits above the info slot, so no info
// toast can push the undo away. Dismissing the undo toast only hides it; the
// clear still commits when its window ends.
export function Toasts({ pending, onUndo, toast, onDismissToast }: Props) {
  const [hiddenSeq, setHiddenSeq] = useState<number | null>(null);
  return (
    <div
      role="status"
      aria-live="polite"
      className="pointer-events-none fixed bottom-12 left-1/2 z-[70] grid -translate-x-1/2 justify-items-center gap-2"
    >
      {pending && pending.seq !== hiddenSeq && (
        <Card
          key={`undo-${pending.seq}`}
          testId="undo-toast"
          message={pending.message}
          action={{ label: "Undo", run: onUndo }}
          onDismiss={() => setHiddenSeq(pending.seq)}
        />
      )}
      {toast && (
        <Card
          key={`toast-${toast.seq}`}
          testId="toast"
          message={toast.message}
          onDismiss={onDismissToast}
        />
      )}
    </div>
  );
}

function Card({
  testId,
  message,
  action,
  onDismiss,
}: {
  testId: string;
  message: string;
  action?: { label: string; run: () => void };
  onDismiss: () => void;
}) {
  const [entered, onAnimationEnd] = useEntered(RISE_MS);
  return (
    <div
      data-testid={testId}
      data-entered={entered ? "true" : undefined}
      onAnimationEnd={onAnimationEnd}
      className="toast"
    >
      <span>{message}</span>
      {action && (
        <button className="btn" onClick={action.run}>
          {action.label}
        </button>
      )}
      <button className="icon-btn icon-btn-sm" aria-label="Dismiss" onClick={onDismiss}>
        <X size={14} />
      </button>
    </div>
  );
}
