import { useEffect, useMemo, useRef } from "react";
import { filterKeyOf } from "../../lib/filters";
import { focusIsFree } from "../../lib/focus";
import { queuePlace } from "../../lib/copy";
import { formatAdded, formatLeft, host } from "../../lib/format";
import type { Modifiers } from "../../lib/selection";
import { sizeText } from "../../lib/rowView";
import type { DownloadSnapshot } from "../../lib/types";
import { useLimitEditor } from "../../lib/useLimitEditor";
import { Grip } from "../icons/Grip";
import { ProgressCell } from "./ProgressCell";
import { RowActions } from "./RowActions";
import type { RowHandlers } from "./rowHandlers";
import { SpeedCell } from "./SpeedCell";
import { StatusGlyph } from "./StatusGlyph";
import { PANE_ID } from "../details/tabs";

type Props = {
  d: DownloadSnapshot;
  /** Finished this session and not opened yet: faint tint and a solid Open. */
  lingering: boolean;
  /** When this row's fresh tint started (ms since epoch), while it runs. */
  freshAt: number | undefined;
  handlers: RowHandlers;
  /** Take focus once the stop just confirmed for this row lands. */
  refocus: boolean;
  onRefocused: () => void;
  selected: boolean;
  /** Shown in the details pane. */
  details: boolean;
  /** The keyboard's row (outlined while the table has focus). */
  cursor: boolean;
  /** Place in the waiting queue (0 starts next), or -1 when not waiting. */
  queueIndex: number;
  /** How a queue drag draws this row, if one is under way. */
  drag: "dragging" | "drop-before" | "drop-after" | null;
  onSelect: (id: number, mods: Modifiers) => void;
  /** The filename button: open or close this row's details. */
  onDetails: (id: number, viaPointer: boolean) => void;
  onMenu: (id: number, x: number, y: number) => void;
  onGripDown: (e: React.PointerEvent<HTMLButtonElement>, id: number) => void;
};

/** Clicks on these belong to the control, not to selecting the row. */
const CONTROLS = "button, input, select, textarea, a, label, .c-act";

const connectionsLabel = (n: number) => `${n} connection${n === 1 ? "" : "s"}`;

// One download. The data-* attributes are the e2e contract (read by gui/e2e/helpers.ts).
export function TableRow({
  d,
  lingering,
  freshAt,
  handlers,
  refocus,
  onRefocused,
  selected,
  cursor,
  queueIndex,
  drag,
  details,
  onSelect,
  onDetails,
  onMenu,
  onGripDown,
}: Props) {
  const editor = useLimitEditor(d);
  const ref = useRef<HTMLTableRowElement>(null);
  const downloading = d.status === "downloading";
  const fresh = lingering && freshAt != null;
  // Fixed per mount: re-setting animation-delay on a running animation re-times it,
  // so a fresh value every render would run the tint ahead of the clock.
  const freshDelay = useMemo(
    () => (freshAt == null ? undefined : `${-(Date.now() - freshAt) / 1000}s`),
    [freshAt],
  );

  // Opening the pane shrinks the table, so keep the row it shows in view.
  useEffect(() => {
    if (details) ref.current?.scrollIntoView({ block: "nearest" });
  }, [details]);

  // A keep-stop lands asynchronously, so focus moves once the row shows Stopped.
  useEffect(() => {
    if (!refocus || d.status !== "cancelled") return;
    if (focusIsFree()) ref.current?.querySelector<HTMLButtonElement>(".c-act button")?.focus();
    onRefocused();
  }, [d.status, refocus, onRefocused]);

  // Clicks on the row's own controls (or anywhere in its actions cell) act, not select.
  const onControl = (e: React.MouseEvent) => (e.target as HTMLElement).closest(CONTROLS) != null;

  return (
    <tr
      ref={ref}
      id={`row-${d.id}`}
      aria-selected={selected}
      data-testid="download-row"
      data-id={d.id}
      data-filename={d.filename}
      data-status={d.status}
      data-limit={d.limit ?? ""}
      data-connections={downloading ? d.connections : 0}
      data-linger={lingering ? "true" : undefined}
      data-partial-kept={String(d.partial_kept)}
      data-finished-at={d.finished_at ?? ""}
      data-selected={details ? "true" : undefined}
      data-next-up={queueIndex === 0 ? "true" : undefined}
      className={`row st-${filterKeyOf(d.status)}${lingering ? " linger" : ""}${fresh ? " fresh" : ""}${selected ? " selected" : ""}${cursor ? " cursor" : ""}${drag ? ` ${drag}` : ""}`}
      // A negative delay resumes the tint mid-way when the row remounts (a filter change).
      style={fresh ? ({ "--fresh-delay": freshDelay } as React.CSSProperties) : undefined}
      onMouseDown={(e) => {
        // Shift-click selects a range; without this it also selects text.
        if (e.shiftKey && !onControl(e)) e.preventDefault();
      }}
      onClick={(e) => {
        if (onControl(e)) return;
        onSelect(d.id, { ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey });
      }}
      onDoubleClick={(e) => {
        if (!onControl(e) && d.status === "completed") handlers.open(d);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        onMenu(d.id, e.clientX, e.clientY);
      }}
    >
      <td className="c-name">
        <div className="flex min-w-0 items-center gap-2">
          <StatusGlyph status={d.status} />
          {queueIndex >= 0 && (
            <button
              className="grip"
              aria-label={`Reorder ${d.filename}. Drag, or press Alt+Up and Alt+Down`}
              title="Drag to change which starts next (Alt+Up, Alt+Down)"
              tabIndex={-1}
              onPointerDown={(e) => onGripDown(e, d.id)}
            >
              <Grip size={14} />
            </button>
          )}
          {/* The keyboard way into the details pane; a plain click anywhere else on the row does the same. */}
          <button
            type="button"
            className="fn-btn"
            aria-label={`${d.filename}, details`}
            aria-expanded={details}
            aria-controls={details ? PANE_ID : undefined}
            title={`${d.filename}. Show details`}
            onClick={(e) => {
              // With Ctrl or Shift held the filename selects like the rest of the row.
              if (e.ctrlKey || e.metaKey || e.shiftKey) {
                onSelect(d.id, { ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey });
              } else {
                onDetails(d.id, e.detail > 0);
              }
            }}
          >
            <span className="nm">
              <span className="fn">{d.filename}</span>
              <span className="host">{host(d.url)}</span>
            </span>
          </button>
        </div>
      </td>
      <td className="c-size num">{sizeText(d)}</td>
      <td className="c-prog" title={queueIndex >= 0 ? queuePlace(queueIndex) : undefined}>
        <ProgressCell d={d} nextUp={queueIndex === 0} />
      </td>
      <td className="c-speed num">
        <SpeedCell d={d} editor={editor} />
      </td>
      <td className="c-left num">{downloading ? formatLeft(d.eta_secs) : ""}</td>
      <td className="c-conn num">
        {downloading && (
          <span data-testid="row-connections" aria-label={connectionsLabel(d.connections)}>
            {d.connections}
          </span>
        )}
      </td>
      <td
        className="c-added num"
        title={d.added_at != null ? new Date(d.added_at * 1000).toLocaleString() : undefined}
      >
        {formatAdded(d.added_at)}
      </td>
      <td className="c-act">
        <RowActions d={d} lingering={lingering} editor={editor} handlers={handlers} />
      </td>
    </tr>
  );
}
