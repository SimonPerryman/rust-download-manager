import { useEffect, useRef, useState } from "react";
import * as api from "../../lib/api";
import { NO_LIMIT } from "../../lib/rate";
import {
  appliedDrafts,
  dirtyPatches,
  draftsFrom,
  fieldOf,
  fieldPatch,
  isDirty,
  mergeApplied,
  type SettingsDrafts as Drafts,
  type SettingsField as Field,
} from "../../lib/settings";
import type { ProxyModeName, Settings } from "../../lib/types";
import { RateInput } from "../RateInput";

const PROXY_MODES: { value: ProxyModeName; label: string }[] = [
  { value: "system", label: "Use system proxy" },
  { value: "none", label: "No proxy" },
  { value: "custom", label: "Custom proxy" },
];

const INPUT_CLASS =
  "flex h-7 rounded-md border border-line-strong bg-raised px-2 text-sm transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-hidden disabled:opacity-60 aria-[invalid]:border-danger";

// The engine-backed "Downloads" section of the settings popover. Values are
// read from the engine each time it mounts (the popover opens) and written
// back on blur/Enter; the engine clamps and the field shows what it applied.
// `onApplied` passes each result on, so the status bar's limit follows.
// A proxy edit refused after the popover has closed goes to `onUnsaved` (a
// toast) instead: unnoticed, downloads would go somewhere the user didn't pick.
export function DownloadSettings({
  onApplied,
  onUnsaved,
}: {
  onApplied: (applied: Settings, fields: Field[]) => void;
  onUnsaved: (message: string) => void;
}) {
  const [saved, setSaved] = useState<Settings | null>(null);
  const [drafts, setDrafts] = useState<Drafts>({
    concurrent: "",
    limit: NO_LIMIT,
    connections: "",
    proxyMode: "system",
    proxy: "",
  });
  const [errors, setErrors] = useState<Partial<Record<Field, string>>>({});
  const mounted = useRef(true);

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
  // clobber each other with stale values. `drafts` is passed in when the
  // change is to commit in the same event that makes it (the proxy's mode).
  const commit = async (field: Field, drafts: Drafts = latest.current.drafts) => {
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
      setDrafts((d) => appliedDrafts(field, d, applied));
      setErrors((e) => ({ ...e, [field]: undefined }));
      onApplied(applied, [field]);
    } catch (e) {
      if (mounted.current) setErrors((prev) => ({ ...prev, [field]: String(e) }));
      else if (field === "proxy") onUnsaved(proxyUnsaved(e));
    }
  };

  // Closing the popover unmounts this before a pending edit's blur can land,
  // so flush every pending edit on the way out (best effort), one call per
  // field so an invalid one can't take the others down with it.
  const latest = useRef({ drafts, saved, onApplied, onUnsaved });
  latest.current = { drafts, saved, onApplied, onUnsaved };
  useEffect(() => {
    // Set here too: StrictMode's dev-only remount runs the cleanup below once.
    mounted.current = true;
    return () => {
      mounted.current = false;
      const { drafts, saved, onApplied, onUnsaved } = latest.current;
      if (!saved) return;
      for (const patch of dirtyPatches(drafts, saved)) {
        const field = fieldOf(patch);
        api
          .setSettings(patch)
          .then((applied) => onApplied(applied, [field]))
          .catch((e) => field === "proxy" && onUnsaved(proxyUnsaved(e)));
      }
    };
  }, []);

  // Use system proxy and No proxy apply at once; Custom waits for its URL.
  const chooseProxyMode = (proxyMode: ProxyModeName) => {
    const next = { ...drafts, proxyMode };
    setDrafts(next);
    setErrors((e) => ({ ...e, proxy: undefined }));
    if (proxyMode !== "custom") void commit("proxy", next);
  };

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
      className={`${INPUT_CLASS} w-full tabular-nums`}
    />
  );

  return (
    <div className="space-y-3" data-testid="download-settings">
      <SettingRow label="Concurrent downloads" error={errors.concurrent}>
        {input("concurrent", "Concurrent downloads")}
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
      <SettingRow
        label="Proxy"
        hint={
          drafts.proxyMode === "custom"
            ? "HTTP or SOCKS5. Applies when a download next starts"
            : "Applies when a download next starts"
        }
        error={errors.proxy}
        wide
      >
        <select
          aria-label="Proxy"
          disabled={!saved}
          value={drafts.proxyMode}
          onChange={(e) => chooseProxyMode(e.target.value as ProxyModeName)}
          className={`${INPUT_CLASS} w-40`}
        >
          {PROXY_MODES.map((m) => (
            <option key={m.value} value={m.value}>
              {m.label}
            </option>
          ))}
        </select>
      </SettingRow>
      {drafts.proxyMode === "custom" && (
        <input
          type="text"
          aria-label="Proxy URL"
          aria-invalid={errors.proxy ? true : undefined}
          disabled={!saved}
          value={drafts.proxy}
          placeholder="socks5h://host:1080"
          spellCheck={false}
          autoComplete="off"
          onChange={(e) => setDrafts((d) => ({ ...d, proxy: e.target.value }))}
          onBlur={() => commit("proxy")}
          onKeyDown={(e) => e.key === "Enter" && commit("proxy")}
          className={`${INPUT_CLASS} w-full`}
        />
      )}
    </div>
  );
}

const proxyUnsaved = (e: unknown) => `Proxy not saved. ${String(e)}`;

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
        <p className="text-sm text-ink">{label}</p>
        <div className={`${wide ? "" : "w-24"} shrink-0`}>{children}</div>
      </div>
      {error ? (
        <p className="mt-1 text-xs text-danger-ink">{error}</p>
      ) : (
        hint && <p className="mt-1 text-xs text-g400">{hint}</p>
      )}
    </div>
  );
}
