import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { atBottom, logText, logTime, logView, visibleLines } from "../../lib/log";
import { loadLogDetailed, saveLogDetailed } from "../../lib/prefs";
import type { LogLevel } from "../../lib/types";
import { useDownloadLog } from "../../lib/useDownloadLog";

/** Spoken before a line whose colour marks it, so the level isn't colour alone. */
const LEVEL_WORD: Partial<Record<LogLevel, string>> = { error: "Error: ", warn: "Warning: " };

// Log: the download's recent engine lines, oldest first, following new ones
// while scrolled to the bottom. Concise by default; Detailed adds the
// per-request lines. Mounted only while the tab is open, which is what keeps
// the polling to that time.
export function LogTab({ id }: { id: number }) {
  const lines = useDownloadLog(id);
  const [detailed, setDetailed] = useState(() => loadLogDetailed());
  const shown = useMemo(() => (lines ? visibleLines(lines, detailed) : []), [lines, detailed]);
  const list = useRef<HTMLOListElement>(null);
  const stick = useRef(true);
  const hasList = shown.length > 0;

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
  const toggle = (on: boolean) => {
    setDetailed(on);
    saveLogDetailed(on);
  };
  return (
    <>
      <div className="log-bar">
        <label className="log-detail" title="Also show each request and lane hand-off">
          <input
            type="checkbox"
            data-testid="log-detailed"
            checked={detailed}
            onChange={(e) => toggle(e.target.checked)}
          />
          Detailed
        </label>
      </div>
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
          Only detailed lines so far. Turn on Detailed to see them.
        </p>
      )}
    </>
  );
}
