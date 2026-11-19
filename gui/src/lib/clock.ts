// Times of day as typed and as shown. The engine and saved data are always 24-hour "HH:MM";
// only what the user types and reads follows the OS's 12/24-hour preference.

/** Whether the OS locale shows a 12-hour clock (the `hour` option makes the answer resolve). */
export function osHour12(): boolean {
  try {
    return new Intl.DateTimeFormat(undefined, { hour: "numeric" }).resolvedOptions().hour12 === true;
  } catch {
    return false;
  }
}

let preference: boolean | null = null;

/** Pins the 12/24-hour preference (null: follow the OS); for tests. */
export function setHour12(value: boolean | null): void {
  preference = value;
}

const pad = (n: number) => String(n).padStart(2, "0");

/** "22:00", or "10:00 PM" on a 12-hour clock. `hhmm` is the engine's 24-hour "HH:MM". */
export function timeLabel(hhmm: string, hour12: boolean = preference ?? osHour12()): string {
  if (!hour12) return hhmm;
  const h = Number(hhmm.slice(0, 2));
  return `${h % 12 === 0 ? 12 : h % 12}:${hhmm.slice(3, 5)} ${h < 12 ? "AM" : "PM"}`;
}

/**
 * What typed text means as a 24-hour "HH:MM", or null if it isn't a time: `930`, `0930`,
 * `9:30`, `9.30`, `9` (9:00), `22`, and with am/pm `930pm`, `9:30 pm`, `12am`.
 */
export function normalizeTime(text: string): string | null {
  const m = /^(\d{1,4})(?:[:.](\d{2}))?\s*(?:([ap])\.?m\.?)?$/.exec(text.trim().toLowerCase());
  if (!m) return null;
  const [, lead, colonMinutes, meridiem] = m;
  let hours: number;
  let minutes: number;
  if (colonMinutes !== undefined) {
    if (lead.length > 2) return null;
    hours = Number(lead);
    minutes = Number(colonMinutes);
  } else if (lead.length <= 2) {
    hours = Number(lead);
    minutes = 0;
  } else if (lead.length <= 4) {
    hours = Number(lead.slice(0, -2));
    minutes = Number(lead.slice(-2));
  } else {
    return null;
  }
  if (minutes > 59) return null;
  if (meridiem) {
    if (hours < 1 || hours > 12) return null;
    hours = (hours % 12) + (meridiem === "p" ? 12 : 0);
  }
  return hours < 24 ? `${pad(hours)}:${pad(minutes)}` : null;
}
