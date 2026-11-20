// Schedule rules as the UI words them: names, the plain-sentence summaries, the form's
// checks and when a queue state next flips. The engine owns what a rule means.

import { normalizeTime, timeLabel } from "./clock";
import { checkDays, daysKind, daysName, daysPhrase } from "./days";
import { limitText, type RateUnit } from "./rate";
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

/** A batch a rule names. */
export interface BatchChoice {
  id: number;
  label: string;
}

/** What a rule can be pointed at: the tags, hosts and batches the downloads have now. */
export interface ScopeChoices {
  tags: string[];
  hosts: string[];
  batches: BatchChoice[];
}

/** The form's fields for either kind of rule. */
export interface RuleDraft {
  kind: RuleKind;
  name: string;
  start: string;
  end: string;
  days: Weekday[];
  /** Download window only. */
  finishRunning: boolean;
  /** Speed limit only: the amount as typed, in `limitUnit`. */
  limit: string;
  limitUnit: RateUnit;
  /** What it applies to: any of these tags, hosts and batches; all three empty is everything. */
  scopeTags: string[];
  scopeHosts: string[];
  scopeBatches: BatchChoice[];
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
  limitUnit: "M",
  scopeTags: [],
  scopeHosts: [],
  scopeBatches: [],
};

/** What the form shows when the Type is switched to Speed limit. */
export const NEW_SPEED: RuleDraft = {
  ...NEW_WINDOW,
  kind: "speed",
  start: "08:00",
  end: "18:00",
  days: EVERY_DAY.slice(0, 5),
};

const UNIT_BYTES: Record<RateUnit, number> = { K: 1024, M: 1024 * 1024 };

/** A limit as typed (KB/s or MB/s) to bytes/s, or null if it isn't a number above zero. */
export function limitBytes(text: string, unit: RateUnit = "M"): number | null {
  const t = text.trim();
  if (!/^(\d+\.?\d*|\.\d+)$/.test(t)) return null;
  const bytes = Math.round(Number(t) * UNIT_BYTES[unit]);
  return bytes >= 1 ? bytes : null;
}

/** A saved limit as the form shows it: in MB/s from a megabyte up, else KB/s. */
export function limitFields(bytes: number): Pick<RuleDraft, "limit" | "limitUnit"> {
  const limitUnit: RateUnit = bytes >= UNIT_BYTES.M ? "M" : "K";
  return { limit: String(Number((bytes / UNIT_BYTES[limitUnit]).toFixed(4))), limitUnit };
}

/** The draft with its typed times as "HH:MM" where they mean a time (`930` is 09:30). */
export const withTimes = (d: RuleDraft): RuleDraft => ({
  ...d,
  start: normalizeTime(d.start) ?? d.start,
  end: normalizeTime(d.end) ?? d.end,
});

/** A limit as shown: "5 MB/s", or "512 KB/s" below a megabyte. */
export const limitLabel = (bytes: number): string => limitText(bytes);

/** Whether a rule is a speed entry (the only kind with a limit). */
export const isSpeed = (r: Rule): r is SpeedEntry => "limit" in r;

/** Names trimmed, empties dropped, repeats (ignoring case) kept once. */
export function uniqueNames(names: readonly string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const n of names.map((x) => x.trim())) {
    if (n && !seen.has(n.toLowerCase())) {
      seen.add(n.toLowerCase());
      out.push(n);
    }
  }
  return out;
}

/** The scope a draft's scope fields make; empty lists are left out. */
export function scopeOfDraft(d: RuleDraft): Scope {
  const tags = uniqueNames(d.scopeTags);
  const hosts = uniqueNames(d.scopeHosts.map((h) => h.toLowerCase()));
  return {
    ...(tags.length ? { tags } : {}),
    ...(hosts.length ? { hosts } : {}),
    ...(d.scopeBatches.length
      ? { batches: d.scopeBatches.map((b) => ({ id: b.id, label: b.label })) }
      : {}),
  };
}

/** Whether a scope names nothing, which is every download. */
export const scopeIsAll = (s: Scope | null | undefined): boolean =>
  !s || !(s.tags?.length || s.hosts?.length || s.batches?.length);

type ScopeFields = Pick<RuleDraft, "scopeTags" | "scopeHosts" | "scopeBatches">;

/** The scope fields of a saved scope. */
function scopeFields(s: Scope | undefined): ScopeFields {
  return {
    scopeTags: [...(s?.tags ?? [])],
    scopeHosts: [...(s?.hosts ?? [])],
    scopeBatches: (s?.batches ?? []).map((b) => ({ id: b.id, label: b.label ?? `Batch ${b.id}` })),
  };
}

/** A piece of a sentence: plain text, a name shown as a chip, or "+N more" with the full list. */
export type Part = string | { chip: string } | { more: number; title: string };

/** A sentence's parts as plain text. */
export const partsText = (parts: readonly Part[]): string =>
  parts.map((p) => (typeof p === "string" ? p : "chip" in p ? p.chip : `+${p.more} more`)).join("");

/** Names as chips: "a", "a or b", and from three on the first two and "+N more". */
function nameParts(names: readonly string[]): Part[] {
  if (names.length <= 2)
    return names.flatMap((n, i): Part[] => (i ? [" or ", { chip: n }] : [{ chip: n }]));
  return [
    { chip: names[0] },
    ", ",
    { chip: names[1] },
    " ",
    { more: names.length - 2, title: names.join(", ") },
  ];
}

/** The scope as said after "Downloads": "tagged course-2 or course-3, from example.com". */
export function scopeParts(s: Scope | null | undefined): Part[] {
  const segments: Part[][] = [];
  if (s?.tags?.length) segments.push(["tagged ", ...nameParts(s.tags)]);
  if (s?.batches?.length) {
    const labels = s.batches.map((b) => b.label ?? String(b.id));
    segments.push([labels.length === 1 ? "in batch " : "in batches ", ...nameParts(labels)]);
  }
  if (s?.hosts?.length) segments.push(["from ", ...nameParts(s.hosts)]);
  return segments.flatMap((seg, i): Part[] => (i ? [", ", ...seg] : seg));
}

/** One scope name that no download matches now. */
export interface ScopeWarning {
  kind: "tag" | "host" | "batch";
  text: string;
}

/** The names in a scope that no download has, as the notes that say so. */
export function scopeWarnings(s: Scope | null | undefined, c: ScopeChoices): ScopeWarning[] {
  const has = (list: readonly string[], name: string) =>
    list.some((x) => x.toLowerCase() === name.trim().toLowerCase());
  const out: ScopeWarning[] = [];
  for (const t of s?.tags ?? [])
    if (!has(c.tags, t)) out.push({ kind: "tag", text: `No downloads have the tag ${t}` });
  for (const b of s?.batches ?? [])
    if (!c.batches.some((x) => x.id === b.id))
      out.push({ kind: "batch", text: `No downloads are in batch ${b.label ?? b.id}` });
  for (const h of s?.hosts ?? [])
    if (!has(c.hosts, h)) out.push({ kind: "host", text: `No downloads are from ${h}` });
  return out;
}

/** The distinct hosts (no port, lower case) of some URLs, sorted. */
export function hostsOf(urls: readonly string[]): string[] {
  const hosts = new Set<string>();
  for (const u of urls) {
    try {
      hosts.add(new URL(u).hostname.toLowerCase());
    } catch {
      // not a URL
    }
  }
  return [...hosts].sort();
}

/** A rule's scope when it isn't all downloads; otherwise the key is left out. */
const scoped = (s: Scope): { scope?: Scope } => (scopeIsAll(s) ? {} : { scope: s });

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
    return { ...NEW_SPEED, ...common, ...limitFields(rule.limit) };
  }
  return { ...NEW_WINDOW, ...common, finishRunning: rule.finish_running };
}

/** What stops a draft being saved, or null. */
export function draftProblem(typed: RuleDraft): string | null {
  const d = withTimes(typed);
  const days = checkDays(d.days);
  if (!days.ok) return days.error;
  if (!validTime(d.start) || !validTime(d.end)) return "Times look like 22:00";
  if (d.start === d.end) return "A window can't start and end at the same time";
  if (d.kind === "speed" && limitBytes(d.limit, d.limitUnit) === null)
    return "Enter a speed above 0";
  return null;
}

/** The window rule a draft makes; `enabled` carries over when editing. */
export function ruleOf(d: RuleDraft, enabled = true, id?: number): QueueWindow {
  const name = d.name.trim();
  return {
    ...(id ? { id } : {}),
    ...(name ? { name } : {}),
    enabled,
    window: { start: d.start, end: d.end, days: d.days },
    finish_running: d.finishRunning,
    ...scoped(scopeOfDraft(d)),
  };
}

/** The speed entry a draft makes (it must have passed `draftProblem`). */
export function speedOf(d: RuleDraft, enabled = true, id?: number): SpeedEntry {
  const name = d.name.trim();
  return {
    ...(id ? { id } : {}),
    ...(name ? { name } : {}),
    enabled,
    window: { start: d.start, end: d.end, days: d.days },
    limit: limitBytes(d.limit, d.limitUnit) ?? 1,
    ...scoped(scopeOfDraft(d)),
  };
}

/** Whichever rule a draft makes, by its kind. */
export const ruleFromDraft = (d: RuleDraft, enabled = true, id?: number): Rule =>
  d.kind === "speed" ? speedOf(d, enabled, id) : ruleOf(d, enabled, id);

/** The name shown when none was given. */
export function autoName(start: string, end: string, days: readonly Weekday[]): string {
  const daily = daysKind(days) === "daily";
  if (daily && crossesMidnight(start, end)) return "Overnight only";
  return `${timeLabel(start)} to ${timeLabel(end)}${daily ? "" : `, ${daysName(days)}`}`;
}

/** The name a draft shows greyed while it has none. */
export function draftAutoName(typed: RuleDraft): string {
  const d = withTimes(typed);
  if (d.kind !== "speed") return autoName(d.start, d.end, d.days);
  const bytes = limitBytes(d.limit, d.limitUnit);
  return bytes === null ? "Speed limit" : `${limitLabel(bytes)} limit`;
}

/** A rule's name: the one given, else one made from its times or limit. */
export function ruleName(r: Rule): string {
  const given = r.name?.trim();
  if (given) return given;
  if (isSpeed(r)) return `${limitLabel(r.limit)} limit`;
  return autoName(r.window.start, r.window.end, r.window.days);
}

type SummaryFields = Pick<RuleDraft, "start" | "end" | "days" | "finishRunning"> &
  Partial<Pick<RuleDraft, "kind" | "limit" | "limitUnit">> & { scope?: Scope | null };

/** The plain-sentence summary of a rule, in parts so the names can be shown as chips. */
export function summaryParts(d: SummaryFields): Part[] {
  const when = `from ${timeLabel(d.start)} to ${timeLabel(d.end)} ${daysPhrase(d.days)}`;
  const tail = scopeParts(d.scope);
  const who = (lead: string): Part[] => (tail.length ? [`${lead} `, ...tail] : [lead]);
  if (d.kind === "speed") {
    const bytes = limitBytes(d.limit ?? "", d.limitUnit);
    return [
      ...who("Downloads"),
      ` are limited to ${bytes === null ? "a set speed" : limitLabel(bytes)} ${when}.`,
    ];
  }
  return d.finishRunning
    ? [...who("New downloads"), ` start ${when}. Running ones finish.`]
    : [...who("Downloads"), ` run ${when}, and pause at ${timeLabel(d.end)}.`];
}

/** The plain-sentence summary of a rule. */
export const summary = (d: SummaryFields): string => partsText(summaryParts(d));

/** A draft's summary parts: its own fields, scope included. */
export const draftSummaryParts = (typed: RuleDraft): Part[] => {
  const d = withTimes(typed);
  return summaryParts({ ...d, scope: scopeOfDraft(d) });
};

/** A draft's summary: its own fields, scope included. */
export const draftSummary = (typed: RuleDraft): string => partsText(draftSummaryParts(typed));

/** A rule's row sentence: a window's, or "5 MB/s, 08:00–18:00, weekdays". */
export function ruleSummaryParts(r: Rule): Part[] {
  if (!isSpeed(r))
    return summaryParts({ ...r.window, finishRunning: r.finish_running, scope: r.scope });
  const kind = daysKind(r.window.days);
  const days = kind === "daily" ? "daily" : daysName(r.window.days).toLowerCase();
  const tail = scopeParts(r.scope);
  return [
    `${limitLabel(r.limit)}, ${timeLabel(r.window.start)}–${timeLabel(r.window.end)}, ${kind === "chosen" ? daysName(r.window.days) : days}`,
    ...(tail.length ? [", ", ...tail] : []),
  ];
}

/** A rule's row text as plain text. */
export const ruleSummary = (r: Rule): string => partsText(ruleSummaryParts(r));

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
  // The copy is a new rule, so it starts a history of its own.
  rules.splice(index + 1, 0, { ...structuredClone(rules[index]), id: undefined });
  return { ...s, [listOf(kind)]: rules };
}

const DAY_NAMES = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/**
 * `until` ("YYYY-MM-DDTHH:MM", local) as it reads aloud: "22:00" if it's today,
 * else with its weekday, "Tue 22:00".
 */
export function whenLabel(until: string, now: Date = new Date()): string {
  const [date, hhmm] = until.split("T");
  const time = timeLabel(hhmm);
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
  const general = (r: Rule) => r.enabled && scopeIsAll(r.scope);
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
