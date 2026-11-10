import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  atBottom,
  copiedAnnouncement,
  copyText,
  logText,
  logTime,
  logView,
  visibleLines,
} from "../../lib/log";
import * as api from "../../lib/api";
import { loadLogDetailed, saveLogDetailed } from "../../lib/prefs";
import type { LogLevel } from "../../lib/types";
import { useCopied } from "../../lib/useCopied";
import { useDownloadLog } from "../../lib/useDownloadLog";
import { Check } from "../icons/Check";
import { Copy } from "../icons/Copy";
import { Save } from "../icons/Save";

/** Spoken before a line whose colour marks it, so the level isn't colour alone. */
const LEVEL_WORD: Partial<Record<LogLevel, string>> = { error: "Error: ", warn: "Warning: " };

// Log: the download's recent engine lines, oldest first, following new ones
// while scrolled to the bottom. Simple by default; Detailed adds the
// per-request lines. Mounted only while the tab is open, which is what keeps
// the polling to that time.
export function LogTab({ id, name }: { id: number; name: string }) {
  const lines = useDownloadLog(id);
  const [detailed, setDetailed] = useState(() => loadLogDetailed());
  const shown = useMemo(() => (lines ? visibleLines(lines, detailed) : []), [lines, detailed]);
  const list = useRef<HTMLOListElement>(null);
  const stick = useRef(true);
  const hasList = shown.length > 0;
  // How many lines the last Copy put on the clipboard, while it says so.
  const [copied, markCopied] = useCopied<number>();
  // Whether Save… just wrote the file, and why the last save failed.
  const [saved, markSaved] = useCopied<true>();
  const [saveError, setSaveError] = useState<string | null>(null);
  // What the last Copy or Save tells a screen reader, while it says so.
  const [said, markSaid] = useCopied<string>();

  useEffect(() => {
    const scroller = list.current?.parentElement;
    if (!hasList || !scroller) return;
    const onScroll = () => {
      stick.current = atBottom(scroller);
    };
    scroller.addEventListener("scroll", onScroll, { passive: true });
    return () => scroller.removeEventListener("scroll", onScroll);
  }, [hasList]);

  useLayoutEffect(() => {
    const scroller = list.current?.parentElement;
    if (scroller && stick.current) scroller.scrollTop = scroller.scrollHeight;
  }, [shown]);

  if (lines === null) return null;
  const view = logView(lines, shown);
  if (view === "empty") {
    return (
      <div className="log-empty" data-testid="log-empty">
        <p className="font-medium">Nothing logged yet</p>
        <p className="text-g600">
          Lines show up here while this download runs. rdm keeps a download's recent lines until you
          quit, so one from before a restart shows none.
        </p>
      </div>
    );
  }
  const choose = (on: boolean) => {
    setDetailed(on);
    saveLogDetailed(on);
  };
  // Copy and Save take every buffered line, detail too, whatever the view.
  const copy = () => {
    const n = lines.length;
    void Promise.resolve()
      .then(() => navigator.clipboard.writeText(copyText(lines)))
      .then(() => {
        markCopied(n);
        markSaid(copiedAnnouncement(n));
      })
      .catch(() => {});
  };
  const save = () => {
    setSaveError(null);
    api
      .saveTextFile(`${name}.log`, copyText(lines))
      .then((path) => {
        if (path == null) return;
        markSaved(true);
        markSaid(`Saved to ${path}`);
      })
      .catch((e) => setSaveError(`Couldn't save the log: ${e}`));
  };
  return (
    <>
      <div className="log-bar">
        <div className="seg-sm" role="group" aria-label="Lines shown">
          <button
            type="button"
            data-testid="log-simple"
            aria-pressed={!detailed}
            onClick={() => choose(false)}
          >
            Simple
          </button>
          <button
            type="button"
            data-testid="log-detailed"
            aria-pressed={detailed}
            title="Also show each request and lane hand-off"
            onClick={() => choose(true)}
          >
            Detailed
          </button>
        </div>
        <span className="flex-1" />
        <button
          type="button"
          className={`btn btn-ghost btn-sm${copied != null ? " copied" : ""}`}
          data-testid="log-copy"
          title="Copy all lines, detailed ones too"
          onClick={copy}
        >
          {copied != null ? <Check size={14} strokeWidth={2.5} /> : <Copy size={14} />}
          {copied != null ? "Copied" : "Copy"}
        </button>
        <button
          type="button"
          className={`btn btn-ghost btn-sm${saved ? " copied" : ""}`}
          data-testid="log-save"
          title="Save the log as a file"
          onClick={save}
        >
          {saved ? <Check size={14} strokeWidth={2.5} /> : <Save size={14} />}
          {saved ? "Saved" : "Save…"}
        </button>
        <span className="sr-only" role="status">
          {said ?? ""}
        </span>
      </div>
      {saveError && (
        <p className="mb-2 text-sm text-danger-ink" role="alert" data-testid="log-save-error">
          {saveError}
        </p>
      )}
      {view === "list" ? (
        // Focusable so the keyboard can scroll it: it holds no controls.
        <ol ref={list} className="log" data-testid="log" aria-label="Log lines" tabIndex={0}>
          {shown.map((l, i) => (
            <li key={i} data-level={l.level}>
              <time dateTime={new Date(l.at).toISOString()}>{logTime(l.at)}</time>
              <span>
                {LEVEL_WORD[l.level] && <span className="sr-only">{LEVEL_WORD[l.level]}</span>}
                {logText(l)}
              </span>
            </li>
          ))}
        </ol>
      ) : (
        <p className="text-sm text-g600" data-testid="log-only-detail">
          Only detailed lines so far. Choose Detailed to see them.
        </p>
      )}
    </>
  );
}
