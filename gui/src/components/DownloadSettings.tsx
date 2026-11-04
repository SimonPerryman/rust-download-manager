import { useEffect, useRef, useState } from "react";
import * as api from "../lib/api";
import { NO_LIMIT } from "../lib/rate";
import {
  dirtyPatches,
  draftsFrom,
  fieldPatch,
  isDirty,
  mergeApplied,
  type SettingsDrafts as Drafts,
  type SettingsField as Field,
} from "../lib/settings";
import type { Settings } from "../lib/types";
import { RateInput } from "./RateInput";

// The engine-backed "Downloads" section of the settings popover. Values are
// read from the engine each time it mounts (the popover opens) and written
// back on blur/Enter; the engine clamps and the field shows what it applied.
export function DownloadSettings() {
  const [saved, setSaved] = useState<Settings | null>(null);
  const [drafts, setDrafts] = useState<Drafts>({
    concurrent: "",
    limit: NO_LIMIT,
    connections: "",
  });
  const [errors, setErrors] = useState<Partial<Record<Field, string>>>({});

  useEffect(() => {
    api
      .getSettings()
      .then((s) => {
        setSaved(s);
        setDrafts(draftsFrom(s));
      })
      .catch((e) => setErrors({ concurrent: String(e) }));
  }, []);

  // Sends only `field`, so overlapping commits of different fields can't
  // clobber each other with stale values.
  const commit = async (field: Field) => {
    if (!saved) return;
    if (!isDirty(field, drafts, saved)) {
      setErrors((e) => ({ ...e, [field]: undefined }));
      return;
    }
    const r = fieldPatch(field, drafts);
    if ("error" in r) {
      setErrors((e) => ({ ...e, [field]: r.error }));
      return;
    }
    try {
      const applied = await api.setSettings(r.patch);
      setSaved((prev) => mergeApplied(prev, applied, [field]));
      setDrafts((d) => ({ ...d, [field]: draftsFrom(applied)[field] }));
      setErrors((e) => ({ ...e, [field]: undefined }));
    } catch (e) {
      setErrors((prev) => ({ ...prev, [field]: String(e) }));
    }
  };

  // Closing the popover unmounts this before a pending edit's blur can land,
  // so flush every pending edit on the way out (best effort), one call per
  // field so an invalid one can't take the others down with it.
  const latest = useRef({ drafts, saved });
  latest.current = { drafts, saved };
  useEffect(
    () => () => {
      const { drafts, saved } = latest.current;
      if (!saved) return;
      for (const patch of dirtyPatches(drafts, saved)) {
        api.setSettings(patch).catch(() => {});
      }
    },
    [],
  );

  const input = (field: "concurrent" | "connections", label: string) => (
    <input
      type="text"
      aria-label={label}
      aria-invalid={errors[field] ? true : undefined}
      disabled={!saved}
      value={drafts[field]}
      onChange={(e) => setDrafts((d) => ({ ...d, [field]: e.target.value }))}
      onFocus={(e) => e.currentTarget.select()}
      onBlur={() => commit(field)}
      onKeyDown={(e) => e.key === "Enter" && commit(field)}
      inputMode="numeric"
      className="flex h-8 w-full rounded-md border border-input bg-transparent px-2.5 text-[13px] tabular-nums shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:opacity-60 aria-[invalid]:border-red-500/70"
    />
  );

  return (
    <div className="space-y-2.5" data-testid="download-settings">
      <SettingRow label="Max concurrent downloads" error={errors.concurrent}>
        {input("concurrent", "Max concurrent downloads")}
      </SettingRow>
      <SettingRow
        label="Global speed limit"
        hint="All downloads combined. Empty = no limit."
        error={errors.limit}
        wide
      >
        <RateInput
          label="Global speed limit"
          size="sm"
          value={drafts.limit}
          onChange={(limit) => setDrafts((d) => ({ ...d, limit }))}
          invalid={!!errors.limit}
          disabled={!saved}
          onEnter={() => commit("limit")}
          onLeave={() => commit("limit")}
        />
      </SettingRow>
      <SettingRow
        label="Max connections per download"
        hint="Applies when a download next starts"
        error={errors.connections}
      >
        {input("connections", "Max connections per download")}
      </SettingRow>
    </div>
  );
}

function SettingRow({
  label,
  hint,
  error,
  wide,
  children,
}: {
  label: string;
  hint?: string;
  error?: string;
  /** Room for a number + unit control instead of a bare count. */
  wide?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div>
      <div className="flex items-center justify-between gap-3">
        <p className="px-0.5 text-[13px] text-foreground">{label}</p>
        <div className={`${wide ? "" : "w-24"} shrink-0`}>{children}</div>
      </div>
      {error ? (
        <p className="mt-1 px-0.5 text-xs text-red-600 dark:text-red-400">{error}</p>
      ) : (
        hint && <p className="mt-1 px-0.5 text-xs text-muted-foreground">{hint}</p>
      )}
    </div>
  );
}
