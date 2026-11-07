import { useRef, useState } from "react";
import * as api from "../../lib/api";
import { globalLimitLabel } from "../../lib/copy";
import { NO_LIMIT, composeRate, rateDraft, sameRate, type RateDraft } from "../../lib/rate";
import type { SettingsField } from "../../lib/settings";
import type { Settings } from "../../lib/types";
import { Gauge } from "../icons/Gauge";
import { RateInput } from "../RateInput";

type Props = {
  settings: Settings | null;
  apply: (applied: Settings, fields: SettingsField[]) => void;
};

// The status bar's global speed limit: a button that turns into the rate
// input. Enter or leaving commits; an invalid draft keeps it open; Escape
// cancels. The engine's answer flows back through `apply`.
export function GlobalLimit({ settings, apply }: Props) {
  const [draft, setDraft] = useState<RateDraft | null>(null);
  const [error, setError] = useState<string>();
  const busy = useRef(false);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const limit = settings?.global_limit ?? null;

  const close = (refocus: boolean) => {
    setDraft(null);
    setError(undefined);
    // After the input unmounts, so focus isn't dropped on <body>.
    if (refocus) setTimeout(() => buttonRef.current?.focus());
  };

  const commit = async (value: RateDraft, refocus: boolean) => {
    if (busy.current) return;
    const r = composeRate(value);
    if (!r.ok) return setError(r.error);
    if (sameRate(value, rateDraft(limit))) return close(refocus);
    busy.current = true;
    try {
      apply(await api.setSettings({ globalLimit: r.rate }), ["limit"]);
      close(refocus);
    } catch (e) {
      setError(String(e));
    } finally {
      busy.current = false;
    }
  };

  if (draft == null) {
    return (
      <button
        ref={buttonRef}
        data-testid="status-limit"
        title="Change the speed limit for all downloads"
        onClick={() => setDraft(rateDraft(limit))}
        className={`inline-flex h-6 items-center gap-1 rounded-sm px-2 text-xs font-medium tabular-nums ${
          limit == null ? "text-g600 hover:bg-hover hover:text-ink" : "bg-warn-soft text-warn-ink"
        }`}
      >
        <Gauge size={14} />
        {globalLimitLabel(limit)}
      </button>
    );
  }

  return (
    <span className="inline-flex items-center gap-2">
      {error && <span className="text-danger-ink">{error}</span>}
      <RateInput
        label="Speed limit for all downloads"
        size="sm"
        autoFocus
        value={draft}
        onChange={(next) => {
          setDraft(next);
          setError(undefined);
        }}
        invalid={!!error}
        onEnter={() => commit(draft, true)}
        onLeave={() => commit(draft, false)}
        onEscape={() => close(true)}
        onClear={() => {
          setDraft(NO_LIMIT);
          commit(NO_LIMIT, true);
        }}
      />
    </span>
  );
}
