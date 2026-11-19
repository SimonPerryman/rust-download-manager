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
  /** The batch it was added with (an id its files share), or null. */
  batch: number | null;
  /** The batch's name, set with `batch`. */
  batch_label: string | null;
  /** Paused by an update restart and still paused (Resume all after the update resumes it). */
  paused_for_update: boolean;
  /**
   * A finished download whose file is no longer where it was saved (deleted or moved). Derived by
   * the engine's checks, never saved; the status stays "completed".
   */
  file_missing: boolean;
  /**
   * While held out of the queue for a start time, when it starts ("YYYY-MM-DDTHH:MM", local), or
   * was due to if `start_missed`; null otherwise.
   */
  start_at: string | null;
  /** The start schedule it is in: downloads given one start time together share it. */
  start_group: number | null;
  /** Its start time passed while rdm was closed, so it waits for the user. */
  start_missed: boolean;
  /**
   * While it waits for a closed queue window that applies to it, when that window opens
   * ("YYYY-MM-DDTHH:MM", local); null otherwise.
   */
  window_until: string | null;
  /**
   * The command run after this download failed to start or exited non-zero; the download itself
   * still completed. Never saved.
   */
  command_warning: string | null;
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

/** A link the engine found in pasted text, and how many times it was listed. */
export interface ListLink {
  url: string;
  count: number;
  /** Index into `ParsedList.patterns` of the range that first made this link, if one did. */
  pattern: number | null;
}

/** A `[a-b]` range that expanded into links. */
export interface ListPattern {
  /** The name part before the range (`lesson`), else the host. */
  stem: string;
  count: number;
}

/** A piece of pasted text the engine left out, and why. */
export interface SkippedLine {
  line: string;
  reason:
    | "not_a_link"
    | "unsupported_scheme"
    | "range_in_host"
    | "range_invalid"
    | "range_width_mismatch"
    | "range_reversed"
    | "multiple_ranges"
    | "range_too_big";
  /** For "range_too_big": how many links the range would make. */
  count: number | null;
}

/** What `add_batch` did: the batch it made (if any), how many links went in, and the ones that didn't. */
export interface BatchAdded {
  batch: number | null;
  added: number;
  failed: { url: string; error: string }[];
}

/** The engine's reading of a pasted list (`preview_list`). */
export interface ParsedList {
  links: ListLink[];
  patterns: ListPattern[];
  skipped: SkippedLine[];
}

/** A day of the week as the engine saves it. */
export type Weekday = "mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun";

/** A recurring span of the day on chosen days; it may cross midnight. */
export interface RuleWindow {
  /** "HH:MM". */
  start: string;
  end: string;
  days: Weekday[];
}

/** The downloads a rule applies to; a rule saved without one applies to all. */
export type Scope =
  | { kind: "all" }
  | { kind: "tag"; tag: string }
  | { kind: "host"; host: string }
  | { kind: "batch"; id: number; label?: string };

/** Downloads start only inside the window (`get_schedule`). */
export interface QueueWindow {
  name?: string;
  enabled: boolean;
  window: RuleWindow;
  /** Let running downloads finish when the window closes, rather than pausing them. */
  finish_running: boolean;
  /** Which downloads it gates; absent for all of them. */
  scope?: Scope;
}

/** Inside its window, `limit` replaces the global speed limit. */
export interface SpeedEntry {
  name?: string;
  enabled: boolean;
  window: RuleWindow;
  /** Bytes per second, above zero. */
  limit: number;
  /** Which downloads it limits; absent for all of them (then it replaces the global limit). */
  scope?: Scope;
}

/** The saved rule set. */
export interface Schedule {
  queue_windows: QueueWindow[];
  speed_entries: SpeedEntry[];
}

/**
 * Whether the queue is open and which scheduled speed limit applies, and when each next
 * changes (local "YYYY-MM-DDTHH:MM").
 */
export interface QueueState {
  open: boolean;
  until: string | null;
  /** Bytes/s a speed entry imposes now; null when the global limit applies. */
  limit: number | null;
  limit_until: string | null;
  /** The scheduled limit from `limit_until` on; null is the global limit. */
  limit_then: number | null;
}

/** What a power action does to the machine. */
export type PowerAction = "shutdown" | "sleep" | "hibernate";

/** Run a command after each download (`get_post_actions`); `{path}` and `{name}` fill in per argument. */
export interface CommandAction {
  enabled: boolean;
  /** The executable, started directly, never through a shell. */
  program: string;
  /** One argument per entry. */
  args: string[];
}

/** Both actions that run when downloads finish; global and off by default. */
export interface PostActions {
  command: CommandAction;
  power: { enabled: boolean; action: PowerAction };
}

/** What a switched-on power action is held back by (see the engine's `WaitReason`). */
export type WaitReason =
  | { kind: "running" }
  | { kind: "queued" }
  | { kind: "window" }
  | { kind: "scheduled"; at: string }
  | { kind: "command" };

/** Where the power action stands (see the engine's `PowerStatus`). */
export type PowerStatus =
  | { state: "off" }
  | { state: "ready" }
  | { state: "waiting"; reason: WaitReason }
  | { state: "countdown"; seconds_left: number; action: PowerAction }
  | { state: "failed"; message: string };

/** The engine's word on the post-download actions (`get_actions_status`). */
export interface ActionsStatus {
  power: PowerStatus;
}
