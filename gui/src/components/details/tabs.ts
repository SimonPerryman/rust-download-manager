/** The details pane's element id, for the row control that opens it. */
export const PANE_ID = "details-pane";

/** The details pane's tabs, in strip order. A new tab is one entry here plus its panel in DetailsPane. */
export const PANE_TABS = [
  { key: "overview", label: "Overview" },
  { key: "speed", label: "Speed" },
  { key: "log", label: "Log" },
] as const;

export type PaneTab = (typeof PANE_TABS)[number]["key"];

/** The tabs a download shows: a missing file has no speed to graph (the Log keeps what happened). */
export function visibleTabs(missing: boolean): readonly (typeof PANE_TABS)[number][] {
  return missing ? PANE_TABS.filter((t) => t.key !== "speed") : PANE_TABS;
}

/** The tab to draw: the chosen one, or Overview when the download no longer has it. */
export function effectiveTab(tab: PaneTab, missing: boolean): PaneTab {
  return visibleTabs(missing).some((t) => t.key === tab) ? tab : "overview";
}
