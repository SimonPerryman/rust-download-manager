// The days a schedule rule runs on, as a set. "Daily", "Weekdays" and "Weekends" are only
// names for common sets; the engine names them the same way (see its `Days`).

import type { Weekday } from "./types";

/** The seven days, Monday first. */
export const WEEKDAYS: readonly Weekday[] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

const SHORT: Record<Weekday, string> = {
  mon: "Mon",
  tue: "Tue",
  wed: "Wed",
  thu: "Thu",
  fri: "Fri",
  sat: "Sat",
  sun: "Sun",
};

/** The three-letter name the UI shows. */
export const dayShort = (d: Weekday): string => SHORT[d];

export type DaysKind = "daily" | "weekdays" | "weekends" | "chosen";

/** Why an empty set is refused; the engine says the same. */
export const NO_DAYS = "Pick at least one day";

const sameSet = (days: readonly Weekday[], want: readonly Weekday[]) =>
  days.length === want.length && want.every((d) => days.includes(d));

/** The set in Monday-first order, each day once. */
export function ordered(days: Iterable<Weekday>): Weekday[] {
  const set = new Set(days);
  return WEEKDAYS.filter((d) => set.has(d));
}

/** Which common set `days` is, or "chosen" for any other. */
export function daysKind(days: readonly Weekday[]): DaysKind {
  if (sameSet(days, WEEKDAYS)) return "daily";
  if (sameSet(days, WEEKDAYS.slice(0, 5))) return "weekdays";
  if (sameSet(days, WEEKDAYS.slice(5))) return "weekends";
  return "chosen";
}

/** The set's name, derived from its contents: "Daily", "Weekdays", "Weekends" or "Mon, Tue and Thu". */
export function daysName(days: readonly Weekday[]): string {
  const kind = daysKind(days);
  if (kind === "daily") return "Daily";
  if (kind === "weekdays") return "Weekdays";
  if (kind === "weekends") return "Weekends";
  const names = ordered(days).map(dayShort);
  if (names.length <= 1) return names.join("");
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/** `days` as a place in a sentence: "every day", "on weekdays", "on Mon and Fri". */
export function daysPhrase(days: readonly Weekday[]): string {
  const kind = daysKind(days);
  if (kind === "daily") return "every day";
  if (kind === "weekdays") return "on weekdays";
  if (kind === "weekends") return "on weekends";
  return `on ${daysName(days)}`;
}

/** The set with `day` added or removed. */
export function toggled(days: readonly Weekday[], day: Weekday): Weekday[] {
  return ordered(days.includes(day) ? days.filter((d) => d !== day) : [...days, day]);
}

/** The days as a set, or why they can't be one. */
export function checkDays(days: readonly Weekday[]): { ok: true } | { ok: false; error: string } {
  return days.length === 0 ? { ok: false, error: NO_DAYS } : { ok: true };
}
