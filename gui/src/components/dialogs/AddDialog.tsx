import { useEffect, useMemo, useRef, useState } from "react";
import * as api from "../../lib/api";
import { addLabel, addLinks, linksToAdd, parseLinks, partialAddError } from "../../lib/batch";
import { formatSpeed } from "../../lib/format";
import { loadLastDir, saveLastDir } from "../../lib/prefs";
import { NO_LIMIT, composeRate, exceeds, type RateDraft } from "../../lib/rate";
import { ChevronDown } from "../icons/ChevronDown";
import { Folder } from "../icons/Folder";
import { RateInput } from "../RateInput";
import { LinkPreview } from "./LinkPreview";
import { trapTab } from "./trapTab";

type Props = {
  open: boolean;
  /** Text to start with (a pasted list or a dropped link). */
  initialText?: string;
  /** URLs already in the list, flagged in the preview. */
  existingUrls: ReadonlySet<string>;
  onClose: () => void;
  /** After a successful add of `n` downloads. */
  onAdded: (n: number) => void;
};

// The Add dialog: one link per line, a preview of what was found, and where
// they go (the last folder used, else the OS download dir). Add calls
// `add_download` once per link, in order. The backend owns everything after
// that; this dialog is just a form.
export function AddDialog({ open, initialText = "", existingUrls, onClose, onAdded }: Props) {
  const [text, setText] = useState("");
  const [dest, setDest] = useState("");
  const [checksum, setChecksum] = useState("");
  const [limit, setLimit] = useState<RateDraft>(NO_LIMIT);
  const [moreOpen, setMoreOpen] = useState(false);
  const [globalLimit, setGlobalLimit] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const textRef = useRef<HTMLTextAreaElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  // While links are going in the dialog stays put, so their outcome lands on it.
  const close = () => {
    if (!busy) onClose();
  };

  // Reset each time it opens: the text may arrive pre-filled from a paste or
  // drop, and the destination starts at the folder used last.
  useEffect(() => {
    if (!open) return;
    setError(null);
    setText(initialText);
    setChecksum("");
    setLimit(NO_LIMIT);
    setMoreOpen(false);
    const last = loadLastDir();
    setDest(last ?? "");
    if (last == null) {
      api
        .defaultDownloadDir()
        .then(setDest)
        .catch(() => setDest(""));
    }
    // Only for the "global limit caps this" note; no note if it can't be read.
    api
      .getSettings()
      .then((s) => setGlobalLimit(s.global_limit))
      .catch(() => setGlobalLimit(null));
  }, [open, initialText]);

  // The caret goes after pre-filled text, ready for another line.
  useEffect(() => {
    if (!open) return;
    const el = textRef.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, [open, initialText]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onClose();
      if (e.key === "Tab") trapTab(e, dialogRef.current);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, busy, onClose]);

  const entries = useMemo(() => parseLinks(text, existingUrls), [text, existingUrls]);
  const links = linksToAdd(entries);
  // A checksum names one file, so it only applies to a single link.
  const single = links.length <= 1;

  if (!open) return null;

  const browse = async () => {
    const picked = await api.pickFolder();
    if (picked) setDest(picked);
  };

  const submit = async () => {
    if (busy) return;
    if (links.length === 0) {
      setError("Paste a link to add.");
      return;
    }
    // An error about a field under "More options" must not be hidden there.
    const fail = (message: string) => {
      setError(message);
      if ((single && checksum.trim()) || limit.amount) setMoreOpen(true);
    };
    const rate = composeRate(limit);
    if (!rate.ok) {
      fail(rate.error);
      return;
    }
    setBusy(true);
    setError(null);
    const dir = dest.trim() || null;
    const outcome = await addLinks(links, (url) =>
      api.addDownload(url, dir, (single && checksum.trim()) || null, rate.rate || null),
    );
    setBusy(false);
    const { added, failed } = outcome;
    if (added > 0 && dir) saveLastDir(dir);
    if (failed.length === 0) {
      onAdded(added);
      onClose();
      return;
    }
    // Keep only what didn't go in, so a fix and Add again doesn't add the rest twice.
    if (added > 0) {
      setText(failed.map((f) => f.url).join("\n"));
      fail(partialAddError(outcome));
    } else {
      fail(failed[0].error);
    }
  };

  return (
    <div className="fixed inset-0 z-50">
      <div onClick={close} className="fixed inset-0 bg-[var(--backdrop)]" />
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label="Add downloads"
        className="fixed left-1/2 top-1/2 z-50 w-full max-w-[600px] -translate-x-1/2 -translate-y-1/2 px-4"
      >
        <div className="rounded-lg bg-raised text-ink shadow-dialog">
          <div className="px-6 pt-6">
            <h2 className="text-xl font-semibold tracking-tight">Add downloads</h2>
            <p className="mt-1 text-sm text-g600">
              Paste one link per line. You'll see what rdm found before anything starts.
            </p>
          </div>

          <div className="space-y-4 px-6 py-4">
            <Field label="Links" htmlFor="add-links">
              <textarea
                id="add-links"
                ref={textRef}
                aria-label="Links"
                spellCheck={false}
                rows={4}
                value={text}
                onChange={(e) => setText(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                    e.preventDefault();
                    void submit();
                  }
                }}
                placeholder={"https://example.com/file.iso\nhttps://example.com/another.tar.gz"}
                className="block max-h-40 min-h-20 w-full resize-y rounded-md border border-line-strong bg-raised px-3 py-2 font-mono text-xs leading-5 transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-none"
              />
              <p className="text-xs text-g400">
                Press Ctrl+V anywhere in rdm to open this with your clipboard. Ctrl+Enter adds.
              </p>
            </Field>

            <LinkPreview entries={entries} />

            <Field label="Destination folder" htmlFor="add-dest">
              <div className="flex gap-2">
                <input
                  id="add-dest"
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
              <p className="text-xs text-g400">rdm remembers the last folder you used.</p>
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
              <Field label="SHA-256 checksum" htmlFor="add-checksum">
                <input
                  id="add-checksum"
                  type="text"
                  aria-label="SHA-256 checksum"
                  value={checksum}
                  disabled={!single}
                  onChange={(e) => setChecksum(e.target.value)}
                  placeholder="e3b0c44298fc1c149afbf4c8996fb924…"
                  className="flex h-8 w-full rounded-md border border-line-strong bg-raised px-3 font-mono text-xs transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-none disabled:opacity-45"
                />
                <p className="text-xs text-g400">
                  {single
                    ? "If provided, the file is verified after download and discarded on mismatch."
                    : "A checksum names one file. Add links one at a time to check each."}
                </p>
              </Field>

              <Field label="Speed limit">
                <RateInput label="Speed limit" value={limit} onChange={setLimit} onEnter={submit} />
                <p className="text-xs text-g400">
                  {globalLimit != null && exceeds(limit, globalLimit)
                    ? `The global limit is ${formatSpeed(globalLimit)} for all downloads combined, so ${single ? "this one" : "these"} won't go faster than that.`
                    : single
                      ? "Empty = no limit."
                      : "Applies to each download. Empty = no limit."}
                </p>
              </Field>
            </div>

            {error && <p className="text-xs text-danger-ink">{error}</p>}
          </div>

          <div className="flex items-center justify-end gap-2 rounded-b-lg bg-surface px-6 py-4">
            <button onClick={close} disabled={busy} className="btn btn-ghost">
              Cancel
            </button>
            <button
              onClick={submit}
              disabled={busy || links.length === 0}
              className="btn btn-primary"
            >
              {addLabel(links.length)}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function Field({
  label,
  htmlFor,
  children,
}: {
  label: string;
  htmlFor?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-1">
      <label htmlFor={htmlFor} className="block text-sm font-medium">
        {label}
      </label>
      {children}
    </div>
  );
}
