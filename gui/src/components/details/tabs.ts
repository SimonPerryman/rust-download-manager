/** The details pane's element id, for the row control that opens it. */
export const PANE_ID = "details-pane";

/** The details pane's tabs, in strip order. A new tab is one entry here plus its panel in DetailsPane. */
export const PANE_TABS = [
  { key: "overview", label: "Overview" },
  { key: "speed", label: "Speed" },
  { key: "log", label: "Log" },
] as const;

export type PaneTab = (typeof PANE_TABS)[number]["key"];
