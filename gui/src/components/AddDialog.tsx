import { useEffect, useState } from "react";
import * as api from "../lib/api";
import { Folder } from "./icons";

type Props = {
  open: boolean;
  onClose: () => void;
};

// The Add-download modal. Destination defaults to the OS download dir; Browse
// opens a native folder picker; Add calls `add_download` then closes. The
// backend owns everything after that — this dialog is just a form.
export function AddDialog({ open, onClose }: Props) {
  const [url, setUrl] = useState("");
  const [dest, setDest] = useState("");
  const [checksum, setChecksum] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Prime the destination field with the OS download dir each time it opens,
  // and reset transient state.
  useEffect(() => {
    if (!open) return;
    setError(null);
    setUrl("");
    setChecksum("");
    api
      .defaultDownloadDir()
      .then(setDest)
      .catch(() => setDest(""));
  }, [open]);

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
    setBusy(true);
    setError(null);
    try {
      await api.addDownload(url.trim(), dest.trim() || null, checksum.trim() || null);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50">
      <div
        onClick={onClose}
        className="fixed inset-0 bg-black/60 backdrop-blur-[1px] transition-opacity"
      />
      <div className="fixed left-1/2 top-1/2 z-50 w-full max-w-md -translate-x-1/2 -translate-y-1/2 px-4">
        <div className="rounded-lg border border-border bg-card text-card-foreground shadow-lg">
          <div className="flex flex-col space-y-1.5 px-6 pb-4 pt-6">
            <h2 className="text-lg font-semibold leading-none tracking-tight">Add download</h2>
            <p className="text-sm text-muted-foreground">
              Paste a URL. Destination and checksum are optional.
            </p>
          </div>

          <div className="space-y-4 px-6 pb-2">
            <Field label="URL">
              <input
                autoFocus
                type="text"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && submit()}
                placeholder="https://releases.ubuntu.com/24.04/ubuntu-24.04.iso"
                className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              />
            </Field>

            <Field label="Destination folder" optional>
              <div className="flex gap-2">
                <input
                  type="text"
                  value={dest}
                  onChange={(e) => setDest(e.target.value)}
                  className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                />
                <button
                  onClick={browse}
                  className="inline-flex h-9 shrink-0 items-center gap-1.5 rounded-md border border-input bg-transparent px-3 text-sm font-medium shadow-sm transition-colors hover:bg-accent hover:text-accent-foreground"
                >
                  <Folder size={15} />
                  Browse
                </button>
              </div>
            </Field>

            <Field label="SHA-256 checksum" optional>
              <input
                type="text"
                value={checksum}
                onChange={(e) => setChecksum(e.target.value)}
                placeholder="e3b0c44298fc1c149afbf4c8996fb924…"
                className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 font-mono text-[13px] shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              />
              <p className="text-xs text-muted-foreground">
                If provided, the file is verified after download and discarded on mismatch.
              </p>
            </Field>

            {error && <p className="text-xs text-red-600 dark:text-red-400">{error}</p>}
          </div>

          <div className="flex items-center justify-end gap-2 px-6 pb-6 pt-4">
            <button
              onClick={onClose}
              className="inline-flex h-9 items-center justify-center rounded-md px-4 text-sm font-medium transition-colors hover:bg-accent hover:text-accent-foreground"
            >
              Cancel
            </button>
            <button
              onClick={submit}
              disabled={busy}
              className="inline-flex h-9 items-center justify-center rounded-md bg-brand px-4 text-sm font-medium text-brand-foreground shadow-sm transition-colors hover:bg-brand/90 disabled:opacity-60"
            >
              Add download
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function Field({
  label,
  optional,
  children,
}: {
  label: string;
  optional?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-2">
      <label className="text-sm font-medium leading-none">
        {label} {optional && <span className="font-normal text-muted-foreground">(optional)</span>}
      </label>
      {children}
    </div>
  );
}
