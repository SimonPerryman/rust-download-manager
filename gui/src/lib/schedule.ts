// Schedule rules as the UI words them: names, the plain-sentence summaries, the form's
// checks and when a queue state next flips. The engine owns what a rule means.

import { checkDays, daysKind, daysName, daysPhrase } from "./days";
import { limitText } from "./rate";
import type { QueueState, QueueWindow, Schedule, Scope, SpeedEntry, Weekday } from "./types";

/** `HH:MM`, two digits each, as the engine reads it. */
export function validTime(text: string): boolean {
  const m = /^(\d{2}):(\d{2})$/.exec(text);
  return m !== null && Number(m[1]) < 24 && Number(m[2]) < 60;
}

const minutes = (t: string) => Number(t.slice(0, 2)) * 60 + Number(t.slice(3));

/** A window that ends earlier than it starts runs overnight. */
export const crossesMidnight = (start: string, end: string) => minutes(end) < minutes(start);

/** The kinds of rule: a window the queue runs in, or a window with its own speed limit. */
export type RuleKind = "window" | "speed";

/** Either kind of rule as saved. */
export type Rule = QueueWindow | SpeedEntry;

/** What a rule applies to, as the form's select names it. */
export type ScopeKind = Scope["kind"];

/** The form's fields for either kind of rule. */
export interface RuleDraft {
  kind: RuleKind;
  name: string;
  start: string;
  end: string;
  days: Weekday[];
  /** Download window only. */
  finishRunning: boolean;
  /** Speed limit only: the number of MB/s, as typed. */
  limit: string;
  /** What it applies to; `scopeText` holds the tag or host as typed, `scopeBatch` the batch. */
  scopeKind: ScopeKind;
  scopeText: string;
  scopeBatch: { id: number; label: string } | null;
}

const EVERY_DAY: Weekday[] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

export const NEW_WINDOW: RuleDraft = {
  kind: "window",
  name: "",
  start: "22:00",
  end: "08:00",
  days: EVERY_DAY,
  finishRunning: false,
  limit: "5",
  scopeKind: "all",
  scopeText: "",
  scopeBatch: null,
};

/** What the form shows when the Type is switched to Speed limit. */
export const NEW_SPEED: RuleDraft = {
  ...NEW_WINDOW,
  kind: "speed",
  start: "08:00",
  end: "18:00",
  days: EVERY_DAY.slice(0, 5),
};

const MB = 1024 * 1024;

/** MB/s as typed to bytes/s, or null if it isn't a number above zero. */
export function limitBytes(text: string): number | null {
  const t = text.trim();
  if (!/^(\d+\.?\d*|\.\d+)$/.test(t)) return null;
  const bytes = Math.round(Number(t) * MB);
  return bytes >= 1 ? bytes : null;
}

/** A limit as shown: "5 MB/s", or "512 KB/s" below a megabyte. */
export const limitLabel = (bytes: number): string => limitText(bytes);

/** Whether a rule is a speed entry (the only kind with a limit). */
export const isSpeed = (r: Rule): r is SpeedEntry => "limit" in r;

/** The scope a draft's scope fields make; null while they are incomplete. */
export function scopeOfDraft(d: RuleDraft): Scope | null {
  const text = d.scopeText.trim();
  switch (d.scopeKind) {
    case "all":
      return { kind: "all" };
    case "tag":
      return text ? { kind: "tag", tag: text } : null;
    case "host":
      return text ? { kind: "host", host: text.toLowerCase() } : null;
    case "batch":
      return d.scopeBatch
        ? { kind: "batch", id: d.scopeBatch.id, label: d.scopeBatch.label }
        : null;
  }
}

type ScopeFields = Pick<RuleDraft, "scopeKind" | "scopeText" | "scopeBatch">;

/** The scope fields of a saved scope. */
function scopeFields(s: Scope | undefined): ScopeFields {
  switch (s?.kind) {
    case "tag":
      return { scopeKind: "tag", scopeText: s.tag, scopeBatch: null };
    case "host":
      return { scopeKind: "host", scopeText: s.host, scopeBatch: null };
    case "batch":
      return {
        scopeKind: "batch",
        scopeText: "",
        scopeBatch: { id: s.id, label: s.label ?? `Batch ${s.id}` },
      };
    default:
      return { scopeKind: "all", scopeText: "", scopeBatch: null };
  }
}

/** The scope as said after "Downloads": "tagged course-2", "from example.com", "in batch Course". */
export function scopeTail(s: Scope | null | undefined): string {
  switch (s?.kind) {
    case "tag":
      return `tagged ${s.tag}`;
    case "host":
      return `from ${s.host}`;
    case "batch":
      return `in batch ${s.label ?? s.id}`;
    default:
      return "";
  }
}

/** A rule's scope when it isn't all downloads; otherwise the key is left out. */
const scoped = (s: Scope | null): { scope?: Scope } => (s && s.kind !== "all" ? { scope: s } : {});

/** The draft of an existing rule, for Edit. */
export function draftOf(rule: Rule): RuleDraft {
  const common = {
    name: rule.name ?? "",
    start: rule.window.start,
    end: rule.window.end,
    days: [...rule.window.days],
    ...scopeFields(rule.scope),
  };
  if (isSpeed(rule)) {
    return { ...NEW_SPEED, ...common, limit: String(Number((rule.limit / MB).toFixed(4))) };
  }
  return { ...NEW_WINDOW, ...common, finishRunning: rule.finish_running };
}

/** What stops a draft being saved, or null. */
export function draftProblem(d: RuleDraft): string | null {
  const days = checkDays(d.days);
  if (!days.ok) return days.error;
  if (!validTime(d.start) || !validTime(d.end)) return "Times look like 22:00";
  if (d.start === d.end) return "A window can't start and end at the same time";
  if (d.kind === "speed" && limitBytes(d.limit) === null) return "Enter a speed above 0";
  if (scopeOfDraft(d) === null) {
    return d.scopeKind === "batch" ? "Pick a batch" : `Enter a ${d.scopeKind}`;
  }
  return null;
}

/** The window rule a draft makes; `enabled` carries over when editing. */
export function ruleOf(d: RuleDraft, enabled = true): QueueWindow {
  const name = d.name.trim();
  return {
    ...(name ? { name } : {}),
    enabled,
    window: { start: d.start, end: d.end, days: d.days },
    finish_running: d.finishRunning,
    ...scoped(scopeOfDraft(d)),
  };
}

/** The speed entry a draft makes (it must have passed `draftProblem`). */
export function speedOf(d: RuleDraft, enabled = true): SpeedEntry {
  const name = d.name.trim();
  return {
    ...(name ? { name } : {}),
    enabled,
    window: { start: d.start, end: d.end, days: d.days },
    limit: limitBytes(d.limit) ?? 1,
    ...scoped(scopeOfDraft(d)),
  };
}

/** Whichever rule a draft makes, by its kind. */
export const ruleFromDraft = (d: RuleDraft, enabled = true): Rule =>
  d.kind === "speed" ? speedOf(d, enabled) : ruleOf(d, enabled);

/** The name shown when none was given. */
export function autoName(start: string, end: string, days: readonly Weekday[]): string {
  const daily = daysKind(days) === "daily";
  if (daily && crossesMidnight(start, end)) return "Overnight only";
  return `${start} to ${end}${daily ? "" : `, ${daysName(days)}`}`;
}

/** The name a draft shows greyed while it has none. */
export function draftAutoName(d: RuleDraft): string {
  if (d.kind !== "speed") return autoName(d.start, d.end, d.days);
  const bytes = limitBytes(d.limit);
  return bytes === null ? "Speed limit" : `${limitLabel(bytes)} limit`;
}

/** A rule's name: the one given, else one made from its times or limit. */
export function ruleName(r: Rule): string {
  const given = r.name?.trim();
  if (given) return given;
  if (isSpeed(r)) return `${limitLabel(r.limit)} limit`;
  return autoName(r.window.start, r.window.end, r.window.days);
}

/** The plain-sentence summary of a draft. */
export function summary(
  d: Pick<RuleDraft, "start" | "end" | "days" | "finishRunning"> &
    Partial<Pick<RuleDraft, "kind" | "limit">> & { scope?: Scope | null },
): string {
  const when = `from ${d.start} to ${d.end} ${daysPhrase(d.days)}`;
  const tail = scopeTail(d.scope);
  const who = (lead: string) => (tail ? `${lead} ${tail}` : lead);
  if (d.kind === "speed") {
    const bytes = limitBytes(d.limit ?? "");
    return `${who("Downloads")} are limited to ${bytes === null ? "a set speed" : limitLabel(bytes)} ${when}.`;
  }
  return d.finishRunning
    ? `${who("New downloads")} start ${when}. Running ones finish.`
    : `${who("Downloads")} run ${when}, and pause at ${d.end}.`;
}

/** A draft's summary: its own fields, scope included. */
export const draftSummary = (d: RuleDraft): string => summary({ ...d, scope: scopeOfDraft(d) });

/** A rule's row text: a window's sentence, or "5 MB/s, 08:00–18:00, weekdays". */
export function ruleSummary(r: Rule): string {
  if (!isSpeed(r)) return summary({ ...r.window, finishRunning: r.finish_running, scope: r.scope });
  const kind = daysKind(r.window.days);
  const days = kind === "daily" ? "daily" : daysName(r.window.days).toLowerCase();
  const tail = scopeTail(r.scope);
  return `${limitLabel(r.limit)}, ${r.window.start}–${r.window.end}, ${kind === "chosen" ? daysName(r.window.days) : days}${tail ? `, ${tail}` : ""}`;
}

/** Where a rule sits in the rule set. */
export interface RuleRef {
  kind: RuleKind;
  index: number;
}

/** Every rule as listed: queue windows first, then speed entries, each with its place. */
export function rulesOf(s: Schedule): (RuleRef & { rule: Rule })[] {
  return [
    ...s.queue_windows.map((rule, index) => ({ kind: "window" as const, index, rule })),
    ...s.speed_entries.map((rule, index) => ({ kind: "speed" as const, index, rule })),
  ];
}

const listOf = (kind: RuleKind) => (kind === "speed" ? "speed_entries" : "queue_windows");

/** The rule set with `rule` added, or replacing the one at `index` (of the rule's own kind). */
export function withRule(s: Schedule, rule: Rule, index?: number): Schedule {
  const key = listOf(isSpeed(rule) ? "speed" : "window");
  const rules: Rule[] = [...s[key]];
  if (index === undefined) rules.push(rule);
  else rules[index] = rule;
  return { ...s, [key]: rules };
}

/** The rule set without the rule at `ref`. */
export const withoutRule = (s: Schedule, { kind, index }: RuleRef): Schedule => ({
  ...s,
  [listOf(kind)]: s[listOf(kind)].filter((_, i) => i !== index),
});

/** The rule set with a copy of the rule at `ref` right after it. */
export function withCopy(s: Schedule, { kind, index }: RuleRef): Schedule {
  const rules: Rule[] = [...s[listOf(kind)]];
  rules.splice(index + 1, 0, structuredClone(rules[index]));
  return { ...s, [listOf(kind)]: rules };
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

/** The speed half of the status line: the cap in force, or the one that starts next. */
function speedNote(q: QueueState, now?: Date): string {
  const until = q.limit_until ? whenLabel(q.limit_until, now) : null;
  if (q.limit !== null) {
    const cap = `A ${limitLabel(q.limit)} cap applies`;
    return until ? `${cap} until ${until}.` : `${cap}.`;
  }
  if (until && q.limit_then !== null)
    return `A ${limitLabel(q.limit_then)} cap starts at ${until}.`;
  return "";
}

/** The Schedules view's one-line status. */
export function statusLine(s: Schedule, q: QueueState | null, now?: Date): string {
  if (s.queue_windows.length === 0 && s.speed_entries.length === 0) {
    return "No schedules. Downloads run any time.";
  }
  const general = (r: Rule) => r.enabled && (r.scope?.kind ?? "all") === "all";
  const windows = s.queue_windows.some(general);
  const speeds = s.speed_entries.some(general);
  const anyOn = [...s.queue_windows, ...s.speed_entries].some((r) => r.enabled);
  if (!anyOn) return "All schedules are off. Downloads run any time.";
  // Scoped rules aren't in the queue state, so the line says so instead of implying the rest.
  if (!windows && !speeds) return "Downloads run any time, except those a rule names.";
  if (!q) return "";
  const at = q.until ? whenLabel(q.until, now) : null;
  let queue = "Downloads run any time.";
  if (windows) {
    if (q.open) queue = at ? `Queue open. The window closes at ${at}.` : "Queue open.";
    else queue = at ? `Queue paused. The window opens at ${at}.` : "Queue paused.";
  }
  const scopedOn = [...s.queue_windows, ...s.speed_entries].some((r) => r.enabled && !general(r));
  const note = scopedOn ? " Other rules name some downloads." : "";
  if (!speeds) return queue + note;
  // A cap that starts as the open window closes reads as one sentence.
  const startsAtClose =
    windows &&
    q.open &&
    at &&
    q.limit === null &&
    q.limit_then !== null &&
    q.limit_until === q.until;
  if (startsAtClose)
    return `${queue.slice(0, -1)}, then a ${limitLabel(q.limit_then!)} cap applies.${note}`;
  const cap = speedNote(q, now);
  return (cap ? `${queue} ${cap}` : queue) + note;
}
