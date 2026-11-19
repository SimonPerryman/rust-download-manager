import { useEffect, useMemo, useRef, useState } from "react";
import * as api from "../../lib/api";
import { batchNameFromStem, folderTickLabel, MAX_BATCH_NAME } from "../../lib/batches";
import {
  addLabel,
  addLinks,
  entriesFrom,
  linksToAdd,
  partialAddError,
  type AddOutcome,
  type Known,
} from "../../lib/batch";
import { formatSpeed } from "../../lib/format";
import { loadLastDir, saveLastDir } from "../../lib/prefs";
import { NO_LIMIT, composeRate, exceeds, type RateDraft } from "../../lib/rate";
import { normalizeTime, timeLabel } from "../../lib/clock";
import { startFromText, startsLabel } from "../../lib/startAt";
import { parseTags, tagsError } from "../../lib/tags";
import type { ParsedList } from "../../lib/types";
import { ChevronDown } from "../icons/ChevronDown";
import { Folder } from "../icons/Folder";
import { RateInput } from "../RateInput";
import { LinkPreview, RangeErrors } from "./LinkPreview";
import { trapTab } from "./trapTab";

type Props = {
  open: boolean;
  /** Text to start with (a pasted list or a dropped link). */
  initialText?: string;
  /** URLs already in the list or the archive, flagged in the preview. */
  existingUrls: ReadonlyMap<string, Known>;
  onClose: () => void;
  /** After a successful add of `n` downloads. */
  onAdded: (n: number) => void;
};

// The Add dialog: one link per line, a preview of what was found, and where
// they go (the last folder used, else the OS download dir). One link is
// `add_download`; two or more are one `add_batch`, which names them a batch if
// the form has a name. The backend owns everything after that; this dialog is
// just a form.
export function AddDialog({ open, initialText = "", existingUrls, onClose, onAdded }: Props) {
  const [text, setText] = useState("");
  const [dest, setDest] = useState("");
  const [checksum, setChecksum] = useState("");
  const [tagsText, setTagsText] = useState("");
  const [batchName, setBatchName] = useState("");
  const [makeFolder, setMakeFolder] = useState(true);
  // Once the name has been typed or cleared by hand, a pattern stops filling it in.
  const nameTouched = useRef(false);
  const [limit, setLimit] = useState<RateDraft>(NO_LIMIT);
  const [batchLimit, setBatchLimit] = useState<RateDraft>(NO_LIMIT);
  const [startText, setStartText] = useState("");
  const [moreOpen, setMoreOpen] = useState(false);
  const [globalLimit, setGlobalLimit] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // The engine's reading of `text`; null until it answers or when it refuses the list.
  const [list, setList] = useState<ParsedList | null>(null);
  const [listError, setListError] = useState<string | null>(null);
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
    setTagsText("");
    setBatchName("");
    setMakeFolder(true);
    nameTouched.current = false;
    setLimit(NO_LIMIT);
    setBatchLimit(NO_LIMIT);
    setStartText("");
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

  // The engine owns what counts as a link; a newer paste outruns a slower answer.
  useEffect(() => {
    if (!open) return;
    let current = true;
    api
      .previewList(text)
      .then((l) => {
        if (!current) return;
        setList(l);
        setListError(null);
      })
      .catch((e) => {
        if (!current) return;
        setList(null);
        setListError(String(e));
      });
    return () => {
      current = false;
    };
  }, [open, text]);

  // A pattern names its batch by what comes before the range (lesson[01-12] gives "lesson").
  const stem = list?.patterns[0]?.stem;
  useEffect(() => {
    if (nameTouched.current || !stem) return;
    setBatchName(batchNameFromStem(stem));
  }, [stem]);

  const entries = useMemo(
    () => (list ? entriesFrom(list, existingUrls) : []),
    [list, existingUrls],
  );
  const links = linksToAdd(entries);
  // A checksum names one file, so it only applies to a single link.
  const single = links.length <= 1;
  // A batch needs two or more links and a name; the name is what makes it one.
  const named = !single && batchName.trim() !== "";

  if (!open) return null;

  const browse = async () => {
    const picked = await api.pickFolder();
    if (picked) setDest(picked);
  };

  const submit = async () => {
    if (busy) return;
    if (links.length === 0) {
      setError(listError ?? "Paste a link to add.");
      return;
    }
    // An error about a field under "More options" must not be hidden there.
    const fail = (message: string) => {
      setError(message);
      if (
        (single && checksum.trim()) ||
        limit.amount ||
        (named && batchLimit.amount) ||
        startText.trim()
      ) {
        setMoreOpen(true);
      }
    };
    const rate = composeRate(limit);
    if (!rate.ok) {
      fail(rate.error);
      return;
    }
    const batchRate = named ? composeRate(batchLimit) : rate;
    if (!batchRate.ok) {
      fail(batchRate.error);
      return;
    }
    const startAt = startText.trim() ? startFromText(startText) : null;
    if (startText.trim() && startAt === null) {
      fail("Times look like 22:00");
      return;
    }
    const tags = parseTags(tagsText);
    const tagProblem = tagsError(tags);
    if (tagProblem) {
      setError(tagProblem);
      return;
    }
    setBusy(true);
    setError(null);
    const dir = dest.trim() || null;
    let outcome: AddOutcome;
    try {
      outcome = single
        ? await addLinks(links, (url) =>
            api.addDownload(url, dir, checksum.trim() || null, rate.rate || null, tags, startAt),
          )
        : await api.addBatch(
            links,
            dir,
            rate.rate || null,
            tags,
            named ? batchName.trim() : null,
            named && makeFolder,
            (named && batchRate.rate) || null,
            startAt,
          );
    } catch (e) {
      setBusy(false);
      fail(String(e));
      return;
    }
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
      <div onClick={close} className="fixed inset-0 bg-(--backdrop)" />
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label="Add downloads"
        className="fixed left-1/2 top-1/2 z-50 w-full max-w-[600px] -translate-x-1/2 -translate-y-1/2 px-4"
      >
        <div className="flex max-h-[calc(100vh-2rem)] flex-col rounded-lg bg-raised text-ink shadow-dialog">
          <div className="shrink-0 px-6 pt-6">
            <h2 className="text-xl font-semibold tracking-tight">Add downloads</h2>
            <p className="mt-1 text-sm text-g600">
              Paste one link per line. You'll see what rdm found before anything starts.
            </p>
          </div>

          <div className="min-h-0 space-y-4 overflow-y-auto px-6 py-4">
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
                className="block max-h-40 min-h-20 w-full resize-y rounded-md border border-line-strong bg-raised px-3 py-2 font-mono text-xs leading-5 transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-hidden"
              />
              <RangeErrors entries={entries} />
              <p className="text-xs text-g400">
                Press Ctrl+V anywhere in rdm to open this with your clipboard. Ctrl+Enter adds.
              </p>
            </Field>

            {listError ? (
              <p data-testid="preview-error" className="text-sm text-danger-ink">
                {listError}
              </p>
            ) : (
              <LinkPreview entries={entries} />
            )}

            {!single && (
              <Field label="Batch name" htmlFor="add-batch-name">
                <input
                  id="add-batch-name"
                  type="text"
                  aria-label="Batch name"
                  value={batchName}
                  maxLength={MAX_BATCH_NAME}
                  onChange={(e) => {
                    nameTouched.current = true;
                    setBatchName(e.target.value);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      void submit();
                    }
                  }}
                  placeholder="Optional, e.g. Course files"
                  className="flex h-8 w-full rounded-md border border-line-strong bg-raised px-3 text-sm transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-hidden"
                />
              </Field>
            )}

            <Field label="Destination folder" htmlFor="add-dest">
              <div className="flex gap-2">
                <input
                  id="add-dest"
                  type="text"
                  aria-label="Destination folder"
                  value={dest}
                  onChange={(e) => setDest(e.target.value)}
                  className="flex h-8 w-full rounded-md border border-line-strong bg-raised px-3 text-sm transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-hidden"
                />
                <button onClick={browse} className="btn btn-soft">
                  <Folder />
                  Browse
                </button>
              </div>
              <p className="text-xs text-g400">rdm remembers the last folder you used.</p>
            </Field>

            {named && (
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  data-testid="batch-folder"
                  checked={makeFolder}
                  onChange={(e) => setMakeFolder(e.target.checked)}
                  className="size-4 accent-(--btn)"
                />
                <span className="min-w-0 break-words">{folderTickLabel(batchName, dest)}</span>
              </label>
            )}

            <Field label="Tags" htmlFor="add-tags">
              <input
                id="add-tags"
                type="text"
                aria-label="Tags"
                value={tagsText}
                onChange={(e) => setTagsText(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    void submit();
                  }
                }}
                placeholder="Optional, e.g. work, iso"
                className="flex h-8 w-full rounded-md border border-line-strong bg-raised px-3 text-sm transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-hidden"
              />
              <p className="text-xs text-g400">
                Separate tags with commas. They apply to every link added here.
              </p>
            </Field>

            <div>
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

              {/* Always rendered (just hidden) so `aria-controls` resolves and drafts survive collapsing.
                  It shares a wrapper with the toggle: as space-y's last child, a hidden panel would
                  leave a gap under the toggle. */}
              <div id="add-more-options" hidden={!moreOpen} className="mt-4 space-y-4">
                {single && (
                  <Field label="SHA-256 checksum" htmlFor="add-checksum">
                    <input
                      id="add-checksum"
                      type="text"
                      aria-label="SHA-256 checksum"
                      value={checksum}
                      onChange={(e) => setChecksum(e.target.value)}
                      placeholder="e3b0c44298fc1c149afbf4c8996fb924…"
                      className="flex h-8 w-full rounded-md border border-line-strong bg-raised px-3 font-mono text-xs transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-hidden"
                    />
                    <p className="text-xs text-g400">
                      If provided, the file is verified after download and discarded on mismatch.
                    </p>
                  </Field>
                )}

                <Field label={single ? "Speed limit" : "Speed limit per file"}>
                  <RateInput
                    label="Speed limit"
                    value={limit}
                    onChange={setLimit}
                    onEnter={submit}
                  />
                  <p className="text-xs text-g400">
                    {globalLimit != null && exceeds(limit, globalLimit)
                      ? `The global limit is ${formatSpeed(globalLimit)} for all downloads combined, so ${single ? "this one" : "these"} won't go faster than that.`
                      : "Empty = no limit."}
                  </p>
                </Field>

                <Field label="Start at" htmlFor="add-start-at">
                  <input
                    id="add-start-at"
                    type="text"
                    aria-label="Start at"
                    value={startText}
                    onChange={(e) => setStartText(e.target.value)}
                    onBlur={() => setStartText((t) => normalizeTime(t) ?? t)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        void submit();
                      }
                    }}
                    placeholder={timeLabel("22:00")}
                    className="flex h-8 w-full rounded-md border border-line-strong bg-raised px-3 text-sm transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-hidden"
                  />
                  {startFromText(startText) && (
                    <p data-testid="add-start-at-echo" className="text-xs text-g600">
                      {startsLabel(startFromText(startText)!)}
                    </p>
                  )}
                </Field>

                {named && (
                  <Field label="Speed limit for the batch">
                    <RateInput
                      label="Speed limit for the batch"
                      value={batchLimit}
                      onChange={setBatchLimit}
                      onEnter={submit}
                    />
                    <p className="text-xs text-g400">Empty = no limit.</p>
                  </Field>
                )}
              </div>
            </div>

            {error && <p className="text-xs text-danger-ink">{error}</p>}
          </div>

          <div className="flex shrink-0 items-center justify-end gap-2 rounded-b-lg bg-surface px-6 py-4">
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
