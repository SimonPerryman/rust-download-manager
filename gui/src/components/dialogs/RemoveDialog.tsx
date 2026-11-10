import { useEffect, useRef, useState } from "react";
import { removeBody, removeTitleFor } from "../../lib/copy";
import type { DownloadSnapshot } from "../../lib/types";
import { trapTab } from "./trapTab";

type Props = {
  /** The rows to remove. */
  ds: DownloadSnapshot[];
  onCancel: () => void;
  /** `dontAsk`: the box was ticked, so later removes skip this dialog. */
  onConfirm: (dontAsk: boolean) => void;
};

// "Remove asks first": removing is for good, so it says what goes and what
// stays on disk. "Don't ask again" turns it off; Settings turns it back on.
export function RemoveDialog({ ds, onCancel, onConfirm }: Props) {
  const [dontAsk, setDontAsk] = useState(false);
  const cancelButton = useRef<HTMLButtonElement>(null);
  const dialog = useRef<HTMLDivElement>(null);
  const names = ds.map((d) => d.filename).join(", ");

  useEffect(() => {
    cancelButton.current?.focus();
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
      if (e.key === "Tab") trapTab(e, dialog.current);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onCancel]);

  return (
    <div
      className="fixed inset-0 z-60 grid place-items-center bg-(--backdrop) p-4"
      onClick={(e) => e.target === e.currentTarget && onCancel()}
    >
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby="remove-title"
        data-testid="remove-dialog"
        className="flex w-full max-w-[440px] flex-col rounded-lg bg-raised text-ink shadow-dialog"
      >
        <div className="px-6 pt-6">
          <h2 id="remove-title" className="text-xl font-semibold tracking-tight">
            {removeTitleFor(ds.length)}
          </h2>
          <p className="mt-1 truncate text-g600" title={names}>
            {names}
          </p>
        </div>
        <div className="grid gap-3 px-6 py-4">
          <p className="text-sm">{removeBody(ds)}</p>
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              aria-label="Don't ask again"
              checked={dontAsk}
              onChange={(e) => setDontAsk(e.target.checked)}
            />
            Don't ask again
          </label>
        </div>
        <div className="flex items-center justify-end gap-2 rounded-b-lg bg-surface px-6 py-4">
          <button ref={cancelButton} className="btn btn-ghost" onClick={onCancel}>
            Cancel
          </button>
          <button className="btn btn-danger" onClick={() => onConfirm(dontAsk)}>
            Remove
          </button>
        </div>
      </div>
    </div>
  );
}
