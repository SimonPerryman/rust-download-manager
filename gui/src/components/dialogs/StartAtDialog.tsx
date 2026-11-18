import { useEffect, useRef, useState } from "react";
import { startFromText, startsLabel } from "../../lib/startAt";
import { trapTab } from "./trapTab";

type Props = {
  /** "Start at" or "Reschedule". */
  title: string;
  /** The confirming button's word. */
  button: string;
  /** Applies the chosen time ("YYYY-MM-DDTHH:MM"); a rejection is shown in the dialog. */
  onSubmit: (at: string) => Promise<unknown>;
  onClose: () => void;
};

// Picks a start time for downloads or schedules: a row's Start at…, a schedule's Edit, and the
// Reschedule menu's Pick a time…. `HH:MM` means the next such time, and the field says which day.
export function StartAtDialog({ title, button, onSubmit, onClose }: Props) {
  const dialog = useRef<HTMLDivElement>(null);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const at = startFromText(text);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      if (e.key === "Tab") trapTab(e, dialog.current);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const submit = async () => {
    if (busy) return;
    if (at === null) {
      setError("Times look like 22:00");
      return;
    }
    setBusy(true);
    try {
      await onSubmit(at);
      onClose();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-60 grid place-items-center bg-(--backdrop) p-4"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        data-testid="start-at-dialog"
        className="flex w-full max-w-[360px] flex-col rounded-lg bg-raised text-ink shadow-dialog"
      >
        <div className="space-y-4 px-6 pb-4 pt-6">
          <h2 className="text-xl font-semibold tracking-tight">{title}</h2>
          <div className="space-y-1">
            <label htmlFor="start-at-time" className="block text-sm font-medium">
              Start at
            </label>
            <input
              id="start-at-time"
              type="text"
              aria-label="Start at"
              autoFocus
              value={text}
              placeholder="22:00"
              onChange={(e) => {
                setText(e.target.value);
                setError(null);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  void submit();
                }
              }}
              className="flex h-8 w-full rounded-md border border-line-strong bg-raised px-3 text-sm transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-hidden"
            />
            <p data-testid="start-at-echo" className="min-h-4 text-xs text-g600">
              {at ? startsLabel(at) : ""}
            </p>
          </div>
          {error && <p className="text-xs text-danger-ink">{error}</p>}
        </div>
        <div className="flex items-center justify-end gap-2 rounded-b-lg bg-surface px-6 py-4">
          <button onClick={onClose} className="btn btn-ghost">
            Cancel
          </button>
          <button
            data-testid="start-at-submit"
            onClick={() => void submit()}
            disabled={busy || at === null}
            className="btn btn-primary"
          >
            {button}
          </button>
        </div>
      </div>
    </div>
  );
}
