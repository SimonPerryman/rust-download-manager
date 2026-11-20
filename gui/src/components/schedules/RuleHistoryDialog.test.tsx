// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { HistoryEvent } from "../../lib/types";

const api = vi.hoisted(() => ({ ruleHistory: vi.fn() }));
vi.mock("../../lib/api", () => api);

const { RuleHistoryDialog } = await import("./RuleHistoryDialog");
const { RuleRow } = await import("./RuleRow");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const day = (daysAgo: number, h: number, m = 0) => {
  const d = new Date();
  d.setDate(d.getDate() - daysAgo);
  d.setHours(h, m, 0, 0);
  return d.getTime();
};

const EVENTS: HistoryEvent[] = [
  { at: day(1, 22), kind: "opened", details: "3 files waiting" },
  { at: day(0, 8), kind: "closed", details: "14 files, 12.3 GB" },
  { at: day(0, 6), kind: "missed", details: "Scheduled start missed" },
];

describe("RuleHistoryDialog", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  const open = async (events: HistoryEvent[], onClose = () => {}) => {
    api.ruleHistory.mockReset().mockResolvedValue(events);
    await act(async () =>
      root.render(<RuleHistoryDialog ruleId={4} name="Course night" onClose={onClose} />),
    );
  };

  it("reads the rule's events and groups them by day, newest first", async () => {
    await open(EVENTS);
    expect(api.ruleHistory).toHaveBeenCalledWith(4);
    expect(host.querySelector("h2")?.textContent).toBe("Course night: history");
    const days = [...host.querySelectorAll('[data-testid="history-day"]')];
    expect(days.map((d) => d.querySelector("h3")?.textContent)).toEqual(["Today", "Yesterday"]);
    const today = [...days[0].querySelectorAll('[data-testid="history-row"]')];
    expect(today.map((r) => r.textContent)).toEqual([
      expect.stringContaining("Closed14 files, 12.3 GB"),
      expect.stringContaining("MissedScheduled start missed"),
    ]);
  });

  it("says so when there is nothing", async () => {
    await open([]);
    expect(host.querySelector('[data-testid="rule-history-empty"]')?.textContent).toBe(
      "Nothing yet",
    );
  });

  it("closes on Escape and on Close", async () => {
    const onClose = vi.fn();
    await open(EVENTS, onClose);
    act(() => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    act(() => host.querySelector<HTMLButtonElement>(".btn")!.click());
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});

describe("a rule's menu", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  const rule = {
    id: 4,
    enabled: true,
    window: { start: "22:00", end: "08:00", days: ["mon" as const] },
    finish_running: false,
  };

  const items = (onHistory?: () => void) => {
    act(() =>
      root.render(
        <ul>
          <RuleRow
            rule={rule}
            onToggle={() => {}}
            onEdit={() => {}}
            onHistory={onHistory}
            onDuplicate={() => {}}
            onDelete={() => {}}
          />
        </ul>,
      ),
    );
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="rule-menu-button"]')!.click());
    return [...host.querySelectorAll('[role="menuitem"]')].map((i) => i.textContent);
  };

  it("offers History between Edit and Duplicate when the rule keeps one", () => {
    expect(items(() => {})).toEqual(["Edit", "History", "Duplicate", "Delete"]);
  });

  it("leaves History out for a rule without one", () => {
    expect(items()).toEqual(["Edit", "Duplicate", "Delete"]);
  });
});
