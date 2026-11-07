import { useEffect, useRef, useState } from "react";
import { stopDeleteHint, stopKeepHint } from "../../lib/copy";
import type { DownloadSnapshot } from "../../lib/types";

type Props = {
  /** The live row; App closes the dialog once it's no longer stoppable. */
  d: DownloadSnapshot;
  onCancel: () => void;
  onConfirm: (keep: boolean) => void;
};

// "Stop asks first": keep the partial file to resume later (the default) or
// delete it. Deliberately no entry animation, so its buttons never move under
// a click.
export function StopDialog({ d, onCancel, onConfirm }: Props) {
  const [keep, setKeep] = useState(true);
  const keepRadio = useRef<HTMLInputElement>(null);
  const dialog = useRef<HTMLDivElement>(null);

  useEffect(() => {
    keepRadio.current?.focus();
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
      className="fixed inset-0 z-[60] grid place-items-center bg-[var(--backdrop)] p-4"
      onClick={(e) => e.target === e.currentTarget && onCancel()}
    >
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby="stop-title"
        data-testid="stop-dialog"
        className="flex w-full max-w-[440px] flex-col rounded-lg bg-raised text-ink shadow-dialog"
      >
        <div className="px-6 pt-6">
          <h2 id="stop-title" className="text-xl font-semibold tracking-tight">
            Stop this download?
          </h2>
          <p className="mt-1 truncate text-g600" title={d.filename}>
            {d.filename}
          </p>
        </div>
        <div className="grid gap-1 px-6 py-4">
          <Choice
            inputRef={keepRadio}
            value="keep"
            checked={keep}
            onChange={() => setKeep(true)}
            title="Keep the partial file to resume later"
            hint={stopKeepHint(d.downloaded)}
          />
          <Choice
            value="delete"
            checked={!keep}
            onChange={() => setKeep(false)}
            title="Delete the partial file"
            hint={stopDeleteHint(d.downloaded)}
          />
        </div>
        <div className="flex items-center justify-end gap-2 rounded-b-lg bg-surface px-6 py-4">
          <button className="btn btn-ghost" onClick={onCancel}>
            Cancel
          </button>
          <button
            className={`btn ${keep ? "btn-primary" : "btn-danger"}`}
            onClick={() => onConfirm(keep)}
          >
            {keep ? "Stop" : "Stop and delete"}
          </button>
        </div>
      </div>
    </div>
  );
}

// aria-modal promises the page behind is out of reach, so Tab cycles inside the
// dialog (the checked radio stands for its group, as the browser's Tab does).
function trapTab(e: KeyboardEvent, root: HTMLElement | null) {
  if (!root) return;
  const stops = Array.from(
    root.querySelectorAll<HTMLElement>('input[type="radio"]:checked, button:not([disabled])'),
  );
  if (stops.length === 0) return;
  const first = stops[0];
  const last = stops[stops.length - 1];
  const inside = root.contains(document.activeElement);
  if (e.shiftKey && (document.activeElement === first || !inside)) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && (document.activeElement === last || !inside)) {
    e.preventDefault();
    first.focus();
  }
}

function Choice({
  value,
  checked,
  onChange,
  title,
  hint,
  inputRef,
}: {
  value: "keep" | "delete";
  checked: boolean;
  onChange: () => void;
  title: string;
  hint: string;
  inputRef?: React.Ref<HTMLInputElement>;
}) {
  return (
    <label className="choice">
      <input
        ref={inputRef}
        type="radio"
        name="stop-mode"
        value={value}
        checked={checked}
        onChange={onChange}
      />
      <span className="font-medium">{title}</span>
      <span className="choice-hint col-start-2 text-xs">{hint}</span>
    </label>
  );
}
