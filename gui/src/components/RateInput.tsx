import { RATE_UNITS, sanitizeAmount, type RateDraft, type RateUnit } from "../lib/rate";
import { X } from "./icons";

type Props = {
  value: RateDraft;
  onChange: (d: RateDraft) => void;
  /** Accessible name of the number box; the unit picker is "`label` unit". */
  label: string;
  size?: "sm" | "md";
  invalid?: boolean;
  disabled?: boolean;
  autoFocus?: boolean;
  onEnter?: () => void;
  /** Focus left the whole control (not just moved between its parts). */
  onLeave?: () => void;
  onEscape?: () => void;
  /** Shows a "Remove limit" button while a limit is entered. */
  onClear?: () => void;
};

// The one speed-limit input: a numeric box plus a KB/s / MB/s picker, so a
// malformed rate can't be typed. Callers own the draft, when to commit, and
// any error or hint shown around it.
export function RateInput({
  value,
  onChange,
  label,
  size = "md",
  invalid,
  disabled,
  autoFocus,
  onEnter,
  onLeave,
  onEscape,
  onClear,
}: Props) {
  const sm = size === "sm";
  return (
    <span
      className={`inline-flex items-center gap-1 ${sm ? "" : "w-full"}`}
      onBlur={(e) => {
        if (onLeave && !e.currentTarget.contains(e.relatedTarget as Node | null)) onLeave();
      }}
      onKeyDown={(e) => {
        // Enter on the "Remove limit" button activates it; it isn't a commit.
        if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) onEnter?.();
        else if (e.key === "Escape" && onEscape) {
          // Keep an enclosing dialog/popover from also closing on this key.
          e.stopPropagation();
          onEscape();
        }
      }}
    >
      <span
        className={`flex items-stretch overflow-hidden rounded-md border bg-transparent shadow-sm transition-colors focus-within:ring-1 focus-within:ring-ring ${
          invalid ? "border-red-500/70" : "border-input"
        } ${sm ? "h-7" : "h-9 w-full"} ${disabled ? "opacity-60" : ""}`}
      >
        <input
          type="text"
          inputMode="decimal"
          aria-label={label}
          aria-invalid={invalid ? true : undefined}
          placeholder="No limit"
          disabled={disabled}
          autoFocus={autoFocus}
          value={value.amount}
          onChange={(e) => onChange({ ...value, amount: sanitizeAmount(e.target.value) })}
          onFocus={(e) => e.currentTarget.select()}
          className={`min-w-0 bg-transparent tabular-nums text-foreground placeholder:text-muted-foreground focus-visible:outline-none ${
            sm ? "w-16 px-2 text-[12.5px]" : "flex-1 px-3 text-sm"
          }`}
        />
        <select
          aria-label={`${label} unit`}
          disabled={disabled}
          value={value.unit}
          onChange={(e) => onChange({ ...value, unit: e.target.value as RateUnit })}
          className={`border-l border-input bg-muted text-foreground focus-visible:outline-none ${
            sm ? "px-1 text-[12px]" : "px-2 text-[13px]"
          }`}
        >
          {RATE_UNITS.map((u) => (
            <option key={u.value} value={u.value}>
              {u.label}
            </option>
          ))}
        </select>
      </span>
      {onClear && value.amount !== "" && (
        <button
          type="button"
          title="Remove limit"
          aria-label="Remove limit"
          // Keep focus inside the control so clicking this isn't also a "leave" commit.
          onMouseDown={(e) => e.preventDefault()}
          onClick={onClear}
          className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        >
          <X size={13} />
        </button>
      )}
    </span>
  );
}
