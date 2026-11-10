import { useState } from "react";
import * as api from "../../lib/api";
import { parseTags, tagsError, unionTags } from "../../lib/tags";
import type { DownloadSnapshot } from "../../lib/types";
import { X } from "../icons/X";

type Props = {
  /** The live rows being tagged: one download, or a selection. */
  rows: readonly DownloadSnapshot[];
  /** Focus the input on mount (in a dialog, not in the details pane). */
  autoFocus?: boolean;
};

// The tags of one or more downloads: each tag (on any of them) with a button
// that takes it off all of them, and an input that adds to all of them. Each
// change is an engine call; the rows' snapshots bring the result back.
export function TagEditor({ rows, autoFocus }: Props) {
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const ids = rows.map((d) => d.id);
  const tags = unionTags(rows);

  const add = () => {
    const added = parseTags(draft);
    if (added.length === 0) return;
    const problem = rows.map((d) => tagsError(added, d.tags)).find((e) => e != null) ?? null;
    if (problem) {
      setError(problem);
      return;
    }
    setError(null);
    api
      .addTags(ids, added)
      .then(() => setDraft(""))
      .catch((e) => setError(String(e)));
  };

  const remove = (tag: string) => {
    setError(null);
    api.removeTags(ids, [tag]).catch((e) => setError(String(e)));
  };

  return (
    <div data-testid="tag-editor" className="tag-editor">
      {tags.length > 0 && (
        <ul className="flex flex-wrap gap-1" aria-label="Tags">
          {tags.map((t) => (
            <li key={t} data-testid="tag-chip" data-tag={t} className="tag-chip">
              <span className="truncate">{t}</span>
              <button
                type="button"
                aria-label={`Remove tag ${t}`}
                title={`Remove tag ${t}`}
                onClick={() => remove(t)}
              >
                <X size={12} />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="flex gap-2">
        <input
          type="text"
          aria-label="Add tags"
          autoFocus={autoFocus}
          value={draft}
          onChange={(e) => {
            setDraft(e.target.value);
            setError(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
          placeholder="Add a tag, or several separated by commas"
          className="flex h-8 min-w-0 flex-1 rounded-md border border-line-strong bg-raised px-3 text-sm transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-hidden"
        />
        <button type="button" className="btn btn-soft" onClick={add} disabled={!draft.trim()}>
          Add
        </button>
      </div>
      {error && <p className="text-xs text-danger-ink">{error}</p>}
    </div>
  );
}
