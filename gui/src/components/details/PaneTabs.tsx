import { useRef } from "react";
import { PANE_TABS, type PaneTab } from "./tabs";

type Props = { tab: PaneTab; onTab: (t: PaneTab) => void };

export const tabId = (t: PaneTab) => `pane-tab-${t}`;
export const PANEL_ID = "pane-panel";

// The tab strip: one tab in the Tab order, arrows and Home/End move between them.
export function PaneTabs({ tab, onTab }: Props) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const onKeyDown = (e: React.KeyboardEvent, i: number) => {
    const last = PANE_TABS.length - 1;
    const to =
      e.key === "ArrowRight"
        ? (i + 1) % PANE_TABS.length
        : e.key === "ArrowLeft"
          ? (i + last) % PANE_TABS.length
          : e.key === "Home"
            ? 0
            : e.key === "End"
              ? last
              : null;
    if (to == null) return;
    e.preventDefault();
    onTab(PANE_TABS[to].key);
    refs.current[to]?.focus();
  };
  return (
    <div className="pane-tabs" role="tablist" aria-label="Details">
      {PANE_TABS.map((t, i) => (
        <button
          key={t.key}
          ref={(el) => {
            refs.current[i] = el;
          }}
          id={tabId(t.key)}
          role="tab"
          data-tab={t.key}
          aria-selected={tab === t.key}
          aria-controls={PANEL_ID}
          tabIndex={tab === t.key ? 0 : -1}
          className="tab-btn"
          onClick={() => onTab(t.key)}
          onKeyDown={(e) => onKeyDown(e, i)}
        >
          {t.label}
        </button>
      ))}
    </div>
  );
}
