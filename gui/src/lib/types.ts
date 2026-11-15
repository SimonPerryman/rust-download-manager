// Mirrors the engine's `DownloadSnapshot` (see src/engine/manager/mod.rs). Field
// names and the `status` set are the frozen contract the backend serializes.
// There is intentionally no "retrying" status: the engine retries internally.

export type Status = "queued" | "downloading" | "paused" | "completed" | "failed" | "cancelled";

/** Where a download's checksum check stands; `pending` means a hash is running now. */
export type ChecksumState = "none" | "pending" | "verified" | "mismatch";

/** One byte range `[start, end)` of a download, of which `done` bytes are written. */
export interface Segment {
  start: number;
  end: number;
  done: number;
}

export interface DownloadSnapshot {
  id: number;
  url: string;
  filename: string;
  target: string;
  final_path: string | null;
  status: Status;
  downloaded: number;
  total: number | null;
  bytes_per_sec: number;
  eta_secs: number | null;
  error: string | null;
  added_at: number | null;
  finished_at: number | null;
  /** Per-download speed cap in bytes/s; null is unlimited. */
  limit: number | null;
  /** Live connections in use; 0 unless downloading. */
  connections: number;
  /** Stopped with the partial file kept, so resuming continues it. */
  partial_kept: boolean;
  /** Expected checksum as `sha256:<hex>`, if one was given. */
  checksum: string | null;
  checksum_state: ChecksumState;
  /**
   * The whole segment tiling of the file; kept while paused, failed or stopped with the
   * partial kept (also after a restart), and once done for this session only.
   */
  segments: Segment[];
  /**
   * For each of `segments`, the lane fetching it now (numbered from 1, as the
   * log names it), or null; empty unless downloading.
   */
  lanes: (number | null)[];
  /** The most lanes open at once since it last started from zero (this session). */
  peak_lanes: number;
  /**
   * Milliseconds spent running (not waiting or paused) since it last started from zero;
   * null when unknown (an older build's record, or a run cut short by a crash).
   */
  active_ms: number | null;
  /** Archived: hidden from the working views, kept as history. Only finished downloads. */
  archived: boolean;
  /** User labels, in the order added; unique ignoring case. */
  tags: string[];
  /** Paused by an update restart and still paused (Resume all after the update resumes it). */
  paused_for_update: boolean;
  /**
   * A finished download whose file is no longer where it was saved (deleted or moved). Derived by
   * the engine's checks, never saved; the status stays "completed".
   */
  file_missing: boolean;
}

/** Mirrors the engine's `LogLevel`. */
export type LogLevel = "error" | "warn" | "info" | "debug" | "trace";

/** Mirrors the engine's `LogLine` (see src/engine/download_log.rs): one line a download logged. */
export interface LogLine {
  /** Milliseconds since the Unix epoch. */
  at: number;
  level: LogLevel;
  message: string;
  /** The event's other fields as space-separated `name=value` pairs. */
  fields: string;
}

/** Mirrors the GUI crate's `UpdateInfo` (gui/src-tauri/src/updater/mod.rs): a newer rdm. */
export interface UpdateInfo {
  version: string;
  currentVersion: string;
  /** The release page; opened through `openUpdateNotes`, never directly. */
  notesUrl: string;
  /** False where rdm can't replace itself (a `.deb` install): offer the release page instead. */
  canInstall: boolean;
}

/** When to install a downloaded update. */
export type InstallWhen = "now" | "on-quit";

/** An update's download progress, in bytes. */
export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

/** Mirrors the GUI crate's `UpdatePrefs` (gui/src-tauri/src/updater/prefs.rs). */
export interface UpdatePrefs {
  /** Null until the first-run question is answered. */
  autoCheck: boolean | null;
  /** This app's version. */
  version: string;
  /** The version the previous run was, on the first run after an update. */
  updatedFrom: string | null;
}

/** Mirrors the engine's manager-wide `Settings` (see src/engine/store.rs). */
export interface Settings {
  max_concurrent: number;
  /** Aggregate speed cap in bytes/s; null is unlimited. */
  global_limit: number | null;
  max_connections: number;
  /** Which proxy downloads use; a custom URL comes with its password masked. */
  proxy: ProxySetting;
}

/** The Proxy setting: the system's proxy (the default), none, or a URL. */
export type ProxySetting = { mode: "system" } | { mode: "none" } | { mode: "custom"; url: string };
export type ProxyModeName = ProxySetting["mode"];

/** Other missing downloads the engine found in a relinked file's new folder (not yet relinked). */
export interface SameFolderOffer {
  count: number;
  old_folder: string;
  new_folder: string;
}

/** What Locate… did: whether it relinked, and any offer to relink more from the same folder. */
export interface LocateOutcome {
  relinked: boolean;
  /** The file it was relinked to; null if nothing was. */
  new_path: string | null;
  offer: SameFolderOffer | null;
}

/** What Locate folder… did: how many it relinked, and the ids of those not in the folder. */
export interface FolderOutcome {
  relinked: number;
  not_found: number[];
}

/** Mirrors the engine's `MatchReason` (src/engine/manager/find.rs): why a found file may be the one. */
export type MatchReason = "SameNameAndSize" | "RenamedUniqueSize" | "RenamedChecksumMatch";

/** Mirrors the engine's `FoundFile`: a file that may be a missing download's file. */
export interface FoundFile {
  path: string;
  size: number;
  /** Seconds since the Unix epoch, if the file system says. */
  modified: number | null;
  reason: MatchReason;
}

/** Mirrors the engine's `FindResult`: what one search found for one missing download. */
export interface FindResult {
  id: number;
  /** Best folder first; empty when nothing matched. */
  matches: FoundFile[];
  searched_folders: string[];
  /** Some folder was not fully searched (a depth or entry limit was reached). */
  truncated: boolean;
}

/** Mirrors the engine's `FindEvent`, as the `find-event` event carries it. */
export type FindEvent =
  | {
      kind: "progress";
      search: number;
      folders_done: number;
      folders_total: number;
      current: string | null;
    }
  | { kind: "result"; search: number; result: FindResult }
  | {
      kind: "finished";
      search: number;
      cancelled: boolean;
      searched_folders: string[];
      truncated: boolean;
    };

/** One candidate the user accepted for a missing download. */
export interface FoundChoice {
  id: number;
  path: string;
}
