import { useEffect, useRef, useState } from "react";
import {
  argsFromText,
  argsText,
  commandProblem,
  COMMAND_VARIABLES,
  insertAt,
} from "../../lib/actions";
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
  // An untouched empty field isn't an error yet.
  const [touched, setTouched] = useState(false);
  const dialog = useRef<HTMLDivElement>(null);
  const first = useRef<HTMLInputElement>(null);
  const argsBox = useRef<HTMLTextAreaElement>(null);
  // Until the box has had focus its caret sits at the very start, which would glue the variable
  // onto the first argument; add it as a new last line instead.
  const argsFocused = useRef(false);
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

  // Into Arguments at the caret, replacing any selection, wherever focus was.
  const insert = (token: string) => {
    const box = argsBox.current;
    if (!box) return;
    const focused = argsFocused.current;
    const at = focused ? [box.selectionStart, box.selectionEnd] : [args.length, args.length];
    const lead = !focused && args !== "" && !args.endsWith("\n") ? "\n" : "";
    const next = insertAt(args, at[0], at[1], lead + token);
    setArgs(next.text);
    // The caret can only be placed once the textarea holds the new value.
    requestAnimationFrame(() => {
      box.focus();
      box.setSelectionRange(next.caret, next.caret);
    });
  };

  const submit = async () => {
    setTouched(true);
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
                onChange={(e) => {
                  setProgram(e.target.value);
                  setTouched(true);
                }}
                onBlur={() => setTouched(true)}
                onKeyDown={(e) => e.key === "Enter" && void submit()}
                placeholder={"C:\\Tools\\scan.exe"}
                className={FIELD}
              />
            </Field>

            <Field label="Arguments" htmlFor="cmd-args">
              <textarea
                id="cmd-args"
                ref={argsBox}
                onFocus={() => (argsFocused.current = true)}
                aria-label="Arguments"
                rows={4}
                value={args}
                onChange={(e) => setArgs(e.target.value)}
                placeholder={"One argument per line\n{download_path}"}
                className={`${FIELD} h-auto resize-none py-2 font-mono`}
              />
              <div className="flex items-center gap-2 pt-1 text-xs text-g600">
                <span>Insert:</span>
                {COMMAND_VARIABLES.map((v) => (
                  <button
                    key={v.token}
                    type="button"
                    data-testid={`insert-${v.token.slice(1, -1)}`}
                    title={v.hint}
                    aria-describedby={`cmd-hint-${v.token.slice(1, -1)}`}
                    onClick={() => insert(v.token)}
                    className="btn btn-line btn-sm"
                  >
                    {v.label}
                  </button>
                ))}
                {COMMAND_VARIABLES.map((v) => (
                  <span key={v.token} id={`cmd-hint-${v.token.slice(1, -1)}`} className="sr-only">
                    {v.hint}
                  </span>
                ))}
              </div>
            </Field>

            <p data-testid="command-rights" className="text-xs text-g600">
              Runs with your rights.
            </p>
            {error && <p className="text-xs text-danger-ink">{error}</p>}
          </div>

          <div className="flex shrink-0 items-center gap-2 rounded-b-lg bg-surface px-6 py-4">
            <p
              data-testid="command-problem"
              className={`min-w-0 flex-1 text-sm ${problem && touched ? "text-danger-ink" : "text-g600"}`}
            >
              {touched ? problem : null}
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
