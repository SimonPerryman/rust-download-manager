import { useEffect, useMemo, useRef } from "react";
import { filterKeyOf } from "../../lib/filters";
import { focusIsFree } from "../../lib/focus";
import { formatAdded, formatLeft } from "../../lib/format";
import { sizeText } from "../../lib/rowView";
import type { DownloadSnapshot } from "../../lib/types";
import { useLimitEditor } from "../../lib/useLimitEditor";
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
  /** Shown in the details pane. */
  selected: boolean;
  onSelect: (id: number | null) => void;
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
  onSelect,
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

  // Opening the pane shrinks the table, so keep the selected row in view.
  useEffect(() => {
    if (selected) ref.current?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  // A keep-stop lands asynchronously, so focus moves once the row shows Stopped.
  useEffect(() => {
    if (!refocus || d.status !== "cancelled") return;
    if (focusIsFree()) ref.current?.querySelector<HTMLButtonElement>(".c-act button")?.focus();
    onRefocused();
  }, [d.status, refocus, onRefocused]);

  return (
    <tr
      ref={ref}
      data-testid="download-row"
      data-id={d.id}
      data-filename={d.filename}
      data-status={d.status}
      data-limit={d.limit ?? ""}
      data-connections={downloading ? d.connections : 0}
      data-linger={lingering ? "true" : undefined}
      data-partial-kept={String(d.partial_kept)}
      data-finished-at={d.finished_at ?? ""}
      data-selected={selected ? "true" : undefined}
      className={`row st-${filterKeyOf(d.status)}${lingering ? " linger" : ""}${fresh ? " fresh" : ""}${selected ? " selected" : ""}`}
      onClick={(e) => {
        if (!(e.target as Element).closest(CONTROLS)) onSelect(d.id);
      }}
      // A negative delay resumes the tint mid-way when the row remounts (a filter change).
      style={fresh ? ({ "--fresh-delay": freshDelay } as React.CSSProperties) : undefined}
    >
      <td className="c-name">
        <div className="flex min-w-0 items-center gap-2">
          <StatusGlyph status={d.status} />
          {/* The keyboard way into the details pane; a click anywhere else on the row does the same. */}
          <button
            type="button"
            className="fn-btn"
            aria-label={`${d.filename}, details`}
            aria-expanded={selected}
            aria-controls={selected ? PANE_ID : undefined}
            title={`${d.filename}. Show details`}
            onClick={() => onSelect(selected ? null : d.id)}
          >
            <span className="fn">{d.filename}</span>
          </button>
        </div>
      </td>
      <td className="c-size num">{sizeText(d)}</td>
      <td className="c-prog">
        <ProgressCell d={d} />
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
