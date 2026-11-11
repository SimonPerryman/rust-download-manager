// Pure helpers for the number + unit speed-limit input: keep the box numeric,
// show a stored bytes/s rate as an amount and unit, and join the two back into
// the rate grammar the engine's `parse_rate` accepts. Validation of the rate
// itself stays in the engine; this only refuses what the box can't mean.

/** Binary units, matching `parse_rate`'s `K`/`M` suffixes (×1024). */
export type RateUnit = "K" | "M";

export const RATE_UNITS: { value: RateUnit; label: string }[] = [
  { value: "K", label: "KB/s" },
  { value: "M", label: "MB/s" },
];

/** A speed-limit field's text: an empty amount means no limit. */
export interface RateDraft {
  amount: string;
  unit: RateUnit;
}

export const NO_LIMIT: RateDraft = { amount: "", unit: "K" };

/** Shown instead of sending `0`, which the engine rejects with a suffix the user never typed. */
export const ZERO_RATE = "Enter a speed above 0, or leave it empty for no limit.";
export const NOT_A_SPEED = "Enter a speed, or leave it empty for no limit.";

const UNIT_BYTES: Record<RateUnit, number> = { K: 1024, M: 1024 * 1024 };

/** `text` reduced to digits and at most one decimal point. */
export function sanitizeAmount(text: string): string {
  const digits = text.replace(/[^\d.]/g, "");
  const dot = digits.indexOf(".");
  return dot < 0 ? digits : digits.slice(0, dot + 1) + digits.slice(dot + 1).replace(/\./g, "");
}

/** A stored bytes/s rate as a draft: MB/s from 1 MiB/s up, else KB/s; null (unlimited) → empty. */
export function rateDraft(bytesPerSec: number | null): RateDraft {
  if (bytesPerSec == null || bytesPerSec <= 0) return NO_LIMIT;
  const unit: RateUnit = bytesPerSec >= UNIT_BYTES.M ? "M" : "K";
  const amount = bytesPerSec / UNIT_BYTES[unit];
  // Two decimals, but significant digits for a set limit that would round to 0.
  return {
    amount: String(Number(amount >= 0.01 ? amount.toFixed(2) : amount.toPrecision(2))),
    unit,
  };
}

/**
 * The rate string to send (`"1.5M"`, or `""` for no limit), or the reason
 * the draft can't be sent.
 */
export function composeRate(
  d: RateDraft,
): { ok: true; rate: string } | { ok: false; error: string } {
  const typed = d.amount.trim();
  if (typed === "") return { ok: true, rate: "" };
  let amount = typed.replace(/\.$/, "");
  if (amount === "") return { ok: false, error: NOT_A_SPEED };
  if (amount.startsWith(".")) amount = `0${amount}`;
  if (Number(amount) === 0) return { ok: false, error: ZERO_RATE };
  return { ok: true, rate: `${amount}${d.unit}` };
}

/** Whether two drafts mean the same limit (any empty amount is "no limit"). */
export function sameRate(a: RateDraft, b: RateDraft): boolean {
  if (a.amount === "" || b.amount === "") return a.amount === b.amount;
  return a.amount === b.amount && a.unit === b.unit;
}

/** Whether the draft asks for more than `capBytesPerSec` (null = no cap). */
export function exceeds(d: RateDraft, capBytesPerSec: number | null): boolean {
  if (capBytesPerSec == null || d.amount === "") return false;
  return Number(d.amount) * UNIT_BYTES[d.unit] > capBytesPerSec;
}

/** A set limit as the table shows it: "50 KB/s", or "50K" (the form typed in the editor) when narrow. */
export function limitText(bytesPerSec: number, short = false): string {
  const { amount, unit } = rateDraft(bytesPerSec);
  return short ? `${amount}${unit}` : `${amount} ${unit}B/s`;
}

/** A speed as a bare number in `unit`, to read against a limit set in it: "49.8", "0.96", "512". */
export function speedIn(bytesPerSec: number, unit: RateUnit): string {
  const value = bytesPerSec / UNIT_BYTES[unit];
  if (value < 0.01) return "<0.01";
  return value.toFixed(value < 10 ? 2 : value < 100 ? 1 : 0);
}
