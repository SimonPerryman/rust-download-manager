import { useEffect, useState } from "react";
import * as api from "../../lib/api";
import { formatSpeed } from "../../lib/format";
import { NO_LIMIT, composeRate, exceeds, type RateDraft } from "../../lib/rate";
import { ChevronDown } from "../icons/ChevronDown";
import { Folder } from "../icons/Folder";
import { RateInput } from "../RateInput";

type Props = {
  open: boolean;
  initialUrl?: string;
  onClose: () => void;
};

// The Add-download modal. Destination defaults to the OS download dir; Browse
// opens a native folder picker; checksum and speed limit sit behind "More
// options". Add calls `add_download` then closes. The backend owns everything
// after that — this dialog is just a form.
export function AddDialog({ open, initialUrl = "", onClose }: Props) {
  const [url, setUrl] = useState("");
  const [dest, setDest] = useState("");
  const [checksum, setChecksum] = useState("");
  const [limit, setLimit] = useState<RateDraft>(NO_LIMIT);
  const [moreOpen, setMoreOpen] = useState(false);
  const [globalLimit, setGlobalLimit] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Prime the destination field with the OS download dir each time it opens,
  // and reset transient state (the URL may arrive pre-filled from drop/paste).
  useEffect(() => {
    if (!open) return;
    setError(null);
    setUrl(initialUrl);
    setChecksum("");
    setLimit(NO_LIMIT);
    setMoreOpen(false);
    api
      .defaultDownloadDir()
      .then(setDest)
      .catch(() => setDest(""));
    // Only for the "global limit caps this" note; no note if it can't be read.
    api
      .getSettings()
      .then((s) => setGlobalLimit(s.global_limit))
      .catch(() => setGlobalLimit(null));
  }, [open, initialUrl]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  if (!open) return null;

  const browse = async () => {
    const picked = await api.pickFolder();
    if (picked) setDest(picked);
  };

  const submit = async () => {
    if (!url.trim()) {
      setError("Enter a URL.");
      return;
    }
    // An error about a field under "More options" must not be hidden there.
    const fail = (message: string) => {
      setError(message);
      if (checksum.trim() || limit.amount) setMoreOpen(true);
    };
    const rate = composeRate(limit);
    if (!rate.ok) {
      fail(rate.error);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.addDownload(
        url.trim(),
        dest.trim() || null,
        checksum.trim() || null,
        rate.rate || null,
      );
      onClose();
    } catch (e) {
      fail(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50">
      <div onClick={onClose} className="fixed inset-0 bg-[var(--backdrop)]" />
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Add download"
        className="fixed left-1/2 top-1/2 z-50 w-full max-w-[560px] -translate-x-1/2 -translate-y-1/2 px-4"
      >
        <div className="rounded-lg bg-raised text-ink shadow-dialog">
          <div className="px-6 pt-6">
            <h2 className="text-xl font-semibold tracking-tight">Add download</h2>
            <p className="mt-1 text-sm text-g600">Paste a link to start.</p>
          </div>

          <div className="space-y-4 px-6 py-4">
            <Field label="URL">
              <input
                autoFocus
                type="text"
                aria-label="URL"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && submit()}
                placeholder="https://releases.ubuntu.com/24.04/ubuntu-24.04.iso"
                className="flex h-8 w-full rounded-md border border-line-strong bg-raised px-3 font-mono text-xs transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-none"
              />
            </Field>

            <Field label="Destination folder">
              <div className="flex gap-2">
                <input
                  type="text"
                  aria-label="Destination folder"
                  value={dest}
                  onChange={(e) => setDest(e.target.value)}
                  className="flex h-8 w-full rounded-md border border-line-strong bg-raised px-3 text-sm transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-none"
                />
                <button onClick={browse} className="btn btn-soft">
                  <Folder />
                  Browse
                </button>
              </div>
            </Field>

            <button
              type="button"
              aria-expanded={moreOpen}
              aria-controls="add-more-options"
              onClick={() => setMoreOpen((o) => !o)}
              className="inline-flex items-center gap-1 text-sm font-medium text-g600 hover:text-ink"
            >
              <ChevronDown size={14} className={moreOpen ? "" : "-rotate-90"} />
              More options
            </button>

            {/* Always rendered (just hidden) so `aria-controls` resolves and drafts survive collapsing. */}
            <div id="add-more-options" hidden={!moreOpen} className="space-y-4">
              <Field label="SHA-256 checksum">
                <input
                  type="text"
                  aria-label="SHA-256 checksum"
                  value={checksum}
                  onChange={(e) => setChecksum(e.target.value)}
                  placeholder="e3b0c44298fc1c149afbf4c8996fb924…"
                  className="flex h-8 w-full rounded-md border border-line-strong bg-raised px-3 font-mono text-xs transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-none"
                />
                <p className="text-xs text-g400">
                  If provided, the file is verified after download and discarded on mismatch.
                </p>
              </Field>

              <Field label="Speed limit">
                <RateInput label="Speed limit" value={limit} onChange={setLimit} onEnter={submit} />
                <p className="text-xs text-g400">
                  {globalLimit != null && exceeds(limit, globalLimit)
                    ? `The global limit is ${formatSpeed(globalLimit)} for all downloads combined, so this one won't go faster than that.`
                    : "Empty = no limit."}
                </p>
              </Field>
            </div>

            {error && <p className="text-xs text-danger-ink">{error}</p>}
          </div>

          <div className="flex items-center justify-end gap-2 rounded-b-lg bg-surface px-6 py-4">
            <button onClick={onClose} className="btn btn-ghost">
              Cancel
            </button>
            <button onClick={submit} disabled={busy} className="btn btn-primary">
              Add download
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1">
      <label className="block text-sm font-medium">{label}</label>
      {children}
    </div>
  );
}
