import { useState } from "react";
import type { ActiveSearch } from "../../lib/find";
import {
  foldersProgress,
  foldersSearched,
  possibleMatchesHeading,
  truncatedNote,
} from "../../lib/copy";
import { folderOf } from "../../lib/folders";
import { formatBytes, formatModified } from "../../lib/format";
import type { DownloadSnapshot, FindResult, FoundFile } from "../../lib/types";
import { Folder } from "../icons/Folder";
import { ListRemove } from "../icons/ListRemove";
import { Loader } from "../icons/Loader";
import { MatchBadge } from "./MatchBadge";

/** What the details pane needs to search for a missing download's file and show the answer. */
export type FindControls = {
  /** The search running for this download, if any. */
  searching: ActiveSearch | null;
  /** What the last search found for it. */
  result: FindResult | undefined;
  /** Why the last Locate… was refused, if it was. */
  locateError: string | null;
  onFind: (d: DownloadSnapshot) => void;
  onCancel: () => void;
  onRelink: (d: DownloadSnapshot, file: FoundFile) => void;
  onSkip: (d: DownloadSnapshot) => void;
};

type Props = FindControls & {
  d: DownloadSnapshot;
  onLocate: (d: DownloadSnapshot) => void;
  onForget: (d: DownloadSnapshot) => void;
};

function Meta({ f }: { f: FoundFile }) {
  return (
    <span className="mt-1 flex flex-wrap gap-x-3 text-xs text-g600">
      <span className="tabular-nums">{formatBytes(f.size)}</span>
      {f.modified != null && <span>Modified {formatModified(f.modified)}</span>}
    </span>
  );
}

function Found({ f }: { f: FoundFile }) {
  const name = f.path.split(/[\\/]/).pop() ?? f.path;
  return (
    <span className="block min-w-0">
      <span className="block truncate text-base font-semibold" title={name}>
        {name}
      </span>
      <span className="block break-all font-mono text-xs leading-[18px] text-g600" title={f.path}>
        {folderOf(f.path)}
      </span>
      <Meta f={f} />
    </span>
  );
}

function Searching({
  d,
  s,
  onCancel,
}: {
  d: DownloadSnapshot;
  s: ActiveSearch;
  onCancel: () => void;
}) {
  const pct = s.foldersTotal > 0 ? (s.foldersDone / s.foldersTotal) * 100 : 0;
  return (
    <div className="find-panel" role="status" aria-live="polite" data-testid="find-searching">
      <h3 className="truncate">Searching for {d.filename}</h3>
      <div
        className="find-bar my-4"
        role="progressbar"
        aria-label="Folders searched"
        aria-valuemin={0}
        aria-valuemax={s.foldersTotal}
        aria-valuenow={s.foldersDone}
      >
        <i style={{ width: `${pct}%` }} />
      </div>
      <div className="flex min-w-0 items-center gap-2 text-g600">
        <Loader size={14} className="find-spin" />
        {s.current ? (
          <span className="min-w-0 truncate font-mono text-xs text-ink" title={s.current}>
            {s.current}
          </span>
        ) : (
          <span>Searching</span>
        )}
        {s.foldersTotal > 0 && (
          <span className="ml-auto whitespace-nowrap tabular-nums" data-testid="find-progress">
            {foldersProgress(s.foldersDone, s.foldersTotal)}
          </span>
        )}
      </div>
      <div className="mt-4">
        <button className="btn btn-line" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
}

function Candidates({
  d,
  matches,
  onRelink,
  onSkip,
}: Pick<Props, "d" | "onRelink" | "onSkip"> & { matches: FoundFile[] }) {
  const [picked, setPicked] = useState(matches[0].path);
  const file = matches.find((m) => m.path === picked) ?? matches[0];
  return (
    <div className="find-panel" data-testid="find-candidates">
      <h3>{possibleMatchesHeading(matches.length)}</h3>
      <div className="find-cands" role="radiogroup" aria-label="Candidate files">
        {matches.map((m) => (
          <button
            key={m.path}
            type="button"
            role="radio"
            aria-checked={m.path === file.path}
            className="find-cand"
            onClick={() => setPicked(m.path)}
          >
            <span className="find-radio" />
            <Found f={m} />
            <MatchBadge reason={m.reason} />
          </button>
        ))}
      </div>
      <div className="flex items-center gap-2">
        <button className="btn btn-primary" onClick={() => onRelink(d, file)}>
          Relink
        </button>
        <button className="text-link" onClick={() => onSkip(d)}>
          Skip
        </button>
      </div>
    </div>
  );
}

function OneMatch({
  d,
  file,
  locateError,
  onRelink,
  onLocate,
}: Pick<Props, "d" | "locateError" | "onRelink" | "onLocate"> & { file: FoundFile }) {
  return (
    <div className="find-panel" data-testid="find-match">
      <h3>Match found</h3>
      <div className="find-cands">
        <div className="find-cand">
          <Found f={file} />
          <MatchBadge reason={file.reason} />
        </div>
      </div>
      <div className="flex items-center gap-2">
        <button className="btn btn-primary" onClick={() => onRelink(d, file)}>
          Relink
        </button>
        <button className="btn btn-ghost" onClick={() => onLocate(d)}>
          <Folder size={16} />
          Locate…
        </button>
      </div>
      <LocateError message={locateError} />
    </div>
  );
}

function NoMatch({
  d,
  result,
  locateError,
  onFind,
  onLocate,
  onForget,
}: Pick<Props, "d" | "locateError" | "onFind" | "onLocate" | "onForget"> & { result: FindResult }) {
  const folders = result.searched_folders;
  return (
    <div className="find-panel" data-testid="find-nomatch">
      <h3>No match found</h3>
      {folders.length > 0 && (
        <>
          <p className="mt-1 text-g600">{foldersSearched(folders.length)}</p>
          <ul className="find-folders">
            {folders.map((f) => (
              <li key={f} className="min-w-0 truncate font-mono text-xs text-g600" title={f}>
                {f}
              </li>
            ))}
          </ul>
        </>
      )}
      {result.truncated && <p className="find-note">{truncatedNote}</p>}
      <div className="mt-4 flex items-center gap-2">
        <button className="btn btn-primary" onClick={() => onLocate(d)}>
          <Folder size={16} />
          Locate…
        </button>
        <button className="btn btn-ghost" onClick={() => onFind(d)}>
          Search again
        </button>
        <span className="flex-1" />
        <button className="text-link" onClick={() => onForget(d)}>
          <ListRemove size={14} className="mr-1" />
          Remove from list
        </button>
      </div>
      <LocateError message={locateError} />
    </div>
  );
}

/** Locate… refused the picked file (a different size); said where the user is looking. */
export function LocateError({ message }: { message: string | null }) {
  if (!message) return null;
  return (
    <p className="mt-3 text-sm text-danger-ink" role="alert" data-testid="locate-error">
      {message}
    </p>
  );
}

// What a Find file shows in place of the missing banner: the search, then its answer. Nothing is
// relinked until the user picks Relink.
export function FindPanel(p: Props) {
  if (p.searching) return <Searching d={p.d} s={p.searching} onCancel={p.onCancel} />;
  const result = p.result;
  if (!result) return null;
  if (result.matches.length === 0) return <NoMatch {...p} result={result} />;
  if (result.matches.length === 1) return <OneMatch {...p} file={result.matches[0]} />;
  return (
    <Candidates key={result.matches.map((m) => m.path).join("|")} {...p} matches={result.matches} />
  );
}
