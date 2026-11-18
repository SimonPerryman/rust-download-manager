import { useEffect, useRef, useState } from "react";
import { argsFromText, argsText, commandProblem } from "../../lib/actions";
import type { CommandAction } from "../../lib/types";
import { trapTab } from "../dialogs/trapTab";
import { X } from "../icons/X";
import { Field, FIELD } from "./FormField";

type Props = {
  initial: CommandAction;
  /** Saves the command; rejects with why it was refused. */
  onSubmit: (program: string, args: string[]) => Promise<void>;
  onClose: () => void;
};

// The Edit command form: an executable and its arguments, one per line so none needs quoting.
// The command is started directly, never through a shell.
export function CommandForm({ initial, onSubmit, onClose }: Props) {
  const [program, setProgram] = useState(initial.program);
  const [args, setArgs] = useState(argsText(initial.args));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const dialog = useRef<HTMLDivElement>(null);
  const first = useRef<HTMLInputElement>(null);
  const problem = commandProblem(program);

  useEffect(() => first.current?.focus(), []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onClose();
      if (e.key === "Tab") trapTab(e, dialog.current);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  const submit = async () => {
    if (busy || problem) return;
    setBusy(true);
    setError(null);
    try {
      await onSubmit(program.trim(), argsFromText(args));
    } catch (e) {
      setBusy(false);
      setError(String(e));
    }
  };

  return (
    <div className="fixed inset-0 z-50">
      <div onClick={() => !busy && onClose()} className="fixed inset-0 bg-(--backdrop)" />
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-label="Edit command"
        className="fixed left-1/2 top-1/2 z-50 w-full max-w-[520px] -translate-x-1/2 -translate-y-1/2 px-4"
      >
        <div className="flex max-h-[calc(100vh-2rem)] flex-col rounded-lg bg-raised text-ink shadow-dialog">
          <div className="flex shrink-0 items-start justify-between px-6 pt-6">
            <h2 className="text-xl font-semibold tracking-tight">Edit command</h2>
            <button
              type="button"
              aria-label="Close"
              onClick={onClose}
              className="icon-btn -mr-2 -mt-1"
            >
              <X size={18} />
            </button>
          </div>

          <div className="min-h-0 space-y-5 overflow-y-auto px-6 py-5">
            <Field label="Program" htmlFor="cmd-program">
              <input
                id="cmd-program"
                ref={first}
                type="text"
                aria-label="Program"
                value={program}
                onChange={(e) => setProgram(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && void submit()}
                placeholder="scan.exe"
                className={FIELD}
              />
            </Field>

            <Field label="Arguments" htmlFor="cmd-args">
              <textarea
                id="cmd-args"
                aria-label="Arguments"
                rows={4}
                value={args}
                onChange={(e) => setArgs(e.target.value)}
                placeholder={"One per line\n{path}"}
                className={`${FIELD} h-auto resize-none py-2 font-mono`}
              />
            </Field>

            <p data-testid="command-rights" className="text-xs text-g600">
              Runs with your rights.
            </p>
            {error && <p className="text-xs text-danger-ink">{error}</p>}
          </div>

          <div className="flex shrink-0 items-center gap-2 rounded-b-lg bg-surface px-6 py-4">
            <p
              data-testid="command-problem"
              className={`min-w-0 flex-1 text-sm ${problem ? "text-danger-ink" : "text-g600"}`}
            >
              {problem}
            </p>
            <button type="button" onClick={onClose} disabled={busy} className="btn btn-ghost">
              Cancel
            </button>
            <button
              type="button"
              data-testid="command-submit"
              onClick={submit}
              disabled={busy || problem !== null}
              className="btn btn-primary"
            >
              Save
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
