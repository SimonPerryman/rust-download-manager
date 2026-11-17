// Schedule rules as the UI words them: names, the plain-sentence summaries, the form's
// checks and when a queue state next flips. The engine owns what a rule means.

import { checkDays, daysKind, daysName, daysPhrase } from "./days";
import type { QueueState, QueueWindow, Schedule, Weekday } from "./types";

/** `HH:MM`, two digits each, as the engine reads it. */
export function validTime(text: string): boolean {
  const m = /^(\d{2}):(\d{2})$/.exec(text);
  return m !== null && Number(m[1]) < 24 && Number(m[2]) < 60;
}

const minutes = (t: string) => Number(t.slice(0, 2)) * 60 + Number(t.slice(3));

/** A window that ends earlier than it starts runs overnight. */
export const crossesMidnight = (start: string, end: string) => minutes(end) < minutes(start);

/** The form's fields for a Download window. */
export interface WindowDraft {
  name: string;
  start: string;
  end: string;
  days: Weekday[];
  finishRunning: boolean;
}

export const NEW_WINDOW: WindowDraft = {
  name: "",
  start: "22:00",
  end: "08:00",
  days: ["mon", "tue", "wed", "thu", "fri", "sat", "sun"],
  finishRunning: false,
};

/** The draft of an existing rule, for Edit. */
export function draftOf(rule: QueueWindow): WindowDraft {
  return {
    name: rule.name ?? "",
    start: rule.window.start,
    end: rule.window.end,
    days: [...rule.window.days],
    finishRunning: rule.finish_running,
  };
}

/** What stops a draft being saved, or null. */
export function draftProblem(d: WindowDraft): string | null {
  const days = checkDays(d.days);
  if (!days.ok) return days.error;
  if (!validTime(d.start) || !validTime(d.end)) return "Times look like 22:00";
  if (d.start === d.end) return "A window can't start and end at the same time";
  return null;
}

/** The rule a draft makes; `enabled` carries over when editing. */
export function ruleOf(d: WindowDraft, enabled = true): QueueWindow {
  const name = d.name.trim();
  return {
    ...(name ? { name } : {}),
    enabled,
    window: { start: d.start, end: d.end, days: d.days },
    finish_running: d.finishRunning,
  };
}

/** The name shown when none was given. */
export function autoName(start: string, end: string, days: readonly Weekday[]): string {
  const daily = daysKind(days) === "daily";
  if (daily && crossesMidnight(start, end)) return "Overnight only";
  return `${start} to ${end}${daily ? "" : `, ${daysName(days)}`}`;
}

/** A rule's name: the one given, else one made from its times. */
export const ruleName = (r: QueueWindow): string =>
  r.name?.trim() || autoName(r.window.start, r.window.end, r.window.days);

/** The plain-sentence summary of a window. */
export function summary(d: Pick<WindowDraft, "start" | "end" | "days" | "finishRunning">): string {
  const when = `from ${d.start} to ${d.end} ${daysPhrase(d.days)}`;
  return d.finishRunning
    ? `New downloads start ${when}. Running ones finish.`
    : `Downloads run ${when}, and pause at ${d.end}.`;
}

export const ruleSummary = (r: QueueWindow): string =>
  summary({ ...r.window, finishRunning: r.finish_running });

/** The rule set with `rule` added, or replacing the one at `index`. */
export function withRule(s: Schedule, rule: QueueWindow, index?: number): Schedule {
  const rules = [...s.queue_windows];
  if (index === undefined) rules.push(rule);
  else rules[index] = rule;
  return { ...s, queue_windows: rules };
}

/** The rule set without the rule at `index`. */
export const withoutRule = (s: Schedule, index: number): Schedule => ({
  ...s,
  queue_windows: s.queue_windows.filter((_, i) => i !== index),
});

/** The rule set with a copy of the rule at `index` right after it. */
export function withCopy(s: Schedule, index: number): Schedule {
  const rules = [...s.queue_windows];
  rules.splice(index + 1, 0, structuredClone(rules[index]));
  return { ...s, queue_windows: rules };
}

const DAY_NAMES = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/**
 * `until` ("YYYY-MM-DDTHH:MM", local) as it reads aloud: "22:00" if it's today,
 * else with its weekday, "Tue 22:00".
 */
export function whenLabel(until: string, now: Date = new Date()): string {
  const [date, time] = until.split("T");
  const [y, m, d] = date.split("-").map(Number);
  const today = now.getFullYear() === y && now.getMonth() + 1 === m && now.getDate() === d;
  if (today) return time;
  return `${DAY_NAMES[new Date(y, m - 1, d).getDay()]} ${time}`;
}

/** The status bar's note while the queue is closed, else null. */
export function pausedNote(q: QueueState | null, now?: Date): string | null {
  if (!q || q.open) return null;
  return q.until ? `Queue paused until ${whenLabel(q.until, now)}` : "Queue paused";
}

/** The Schedules view's one-line status. */
export function statusLine(s: Schedule, q: QueueState | null, now?: Date): string {
  if (s.queue_windows.length === 0) return "No schedules. Downloads run any time.";
  if (!s.queue_windows.some((r) => r.enabled)) {
    return "All schedules are off. Downloads run any time.";
  }
  if (!q) return "";
  const at = q.until ? whenLabel(q.until, now) : null;
  if (q.open) return at ? `Queue open. The window closes at ${at}.` : "Queue open.";
  return at ? `Queue paused. The window opens at ${at}.` : "Queue paused.";
}
