// Typed wrappers over the Tauri command surface. Every function here is a
// one-line pass-through to a backend command, which is itself a thin wrapper
// over the engine's `Manager`. No download logic lives in the frontend.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SettingsPatch } from "./settings";
import type {
  DownloadSnapshot,
  FindEvent,
  FindResult,
  BatchAdded,
  BatchMatch,
  FolderOutcome,
  FoundChoice,
  InstallWhen,
  LocateOutcome,
  HistoryEvent,
  LogLine,
  ParsedList,
  ActionsStatus,
  PostActions,
  QueueState,
  Schedule,
  Settings,
  Status,
  UpdateInfo,
  UpdatePrefs,
  UpdateProgress,
} from "./types";

/** One-shot snapshot of every download, in display order. */
export const list = () => invoke<DownloadSnapshot[]>("list");

/**
 * Add a download; `dir`/`checksum`/`limit`/`tags` optional (`limit` is a rate
 * like `"500K"`). Returns the new id, or rejects with a parse/validation error.
 */
export const addDownload = (
  url: string,
  dir?: string | null,
  checksum?: string | null,
  limit?: string | null,
  tags?: string[] | null,
  startAt?: string | null,
) =>
  invoke<number>("add_download", {
    url,
    dir: dir ?? null,
    checksum: checksum ?? null,
    limit: limit ?? null,
    tags: tags ?? null,
    startAt: startAt ?? null,
  });

/**
 * Add several links at once. With two or more and a `label` they become a batch (`folder`
 * puts them in a folder of that name inside `dir`); otherwise they are ordinary downloads.
 * `limit` (a rate like `"500K"`) and `tags` apply to each; `batchLimit` caps the batch's items
 * combined (ignored without a label). Rejects on a bad name, tag or rate.
 */
export const addBatch = (
  urls: readonly string[],
  dir: string | null,
  limit: string | null,
  tags: string[],
  label: string | null,
  folder: boolean,
  batchLimit: string | null = null,
  startAt: string | null = null,
) =>
  invoke<BatchAdded>("add_batch", { urls, dir, limit, tags, label, folder, batchLimit, startAt });

/** The existing batch a typed name matches (trimmed, any case), or null. */
export const findBatch = (label: string) => invoke<BatchMatch | null>("find_batch", { label });

/**
 * Add links to an existing batch: its name, folder and shared cap. `limit` and `tags` apply to
 * each link. Rejects when the batch is gone or on a bad tag or rate.
 */
export const addToBatch = (
  batch: number,
  urls: readonly string[],
  limit: string | null,
  tags: string[],
  startAt: string | null = null,
) => invoke<BatchAdded>("add_to_batch", { batch, urls, limit, tags, startAt });

/**
 * Hold downloads out of the queue until a local time ("YYYY-MM-DDTHH:MM", or `HH:MM` for the
 * next one) as one schedule. Resolves with how many could be scheduled; rejects on a bad time.
 */
export const scheduleStart = (ids: number[], at: string) =>
  invoke<number>("schedule_start", { ids, at });

/** Move whole start schedules to a new time. */
export const reschedule = (groups: number[], at: string) =>
  invoke<void>("reschedule", { groups, at });

/** Start schedules now: their downloads join the queue, oldest due first, under its usual cap. */
export const startSchedulesNow = (groups: number[]) =>
  invoke<void>("start_schedules_now", { groups });

/** Drop schedules; their downloads stay in the list, paused. */
export const dismissSchedules = (groups: number[]) => invoke<void>("dismiss_schedules", { groups });

/** Retry the failed downloads of one batch. Resolves with how many were retried. */
export const retryBatch = (batch: number) => invoke<number>("retry_batch", { batch });

/** Open the folder a batch's files went into. By batch id: the backend picks the path. */
export const revealBatch = (batch: number) => invoke<void>("reveal_batch", { batch });

/** The engine's manager-wide settings. */
export const getSettings = () => invoke<Settings>("get_settings");

/**
 * Apply some manager-wide settings; omitted fields are left unchanged
 * (`globalLimit` is a rate string, blank = unlimited; a custom `proxy` URL
 * is checked by the engine). Resolves with the settings as the engine clamped them.
 */
export const setSettings = (patch: SettingsPatch) =>
  invoke<Settings>("set_settings", {
    maxConcurrent: patch.maxConcurrent ?? null,
    globalLimit: patch.globalLimit ?? null,
    maxConnections: patch.maxConnections ?? null,
    proxy: patch.proxy ?? null,
  });

/**
 * Cap one download's speed (`limit` is a rate like `"500K"`; null or blank =
 * unlimited). Applies live. Resolves with the limit the engine stored.
 */
export const setLimit = (id: number, limit: string | null) =>
  invoke<number | null>("set_limit", { id, limit });

export const pause = (id: number) => invoke<void>("pause", { id });
export const resume = (id: number) => invoke<void>("resume", { id });
/** Stop a download, keeping its partial file to resume later or deleting it. */
export const stop = (id: number, keepPartial: boolean) => invoke<void>("stop", { id, keepPartial });
/** Move a waiting download to `index` among the waiting ones. */
export const moveQueued = (id: number, index: number) => invoke<void>("move_queued", { id, index });
/**
 * Archive finished downloads by id; others are left as they are. Resolves with the
 * ids archived, for Undo to unarchive.
 */
export const archive = (ids: number[]) => invoke<number[]>("archive", { ids });
/** Return archived downloads to the working views. Resolves with the count changed. */
export const unarchive = (ids: number[]) => invoke<number>("unarchive", { ids });
/**
 * Archive finished downloads in `statuses` (only `ids`, if given). Resolves with the
 * ids archived, for Undo to unarchive.
 */
export const archiveFinished = (statuses: Status[], ids?: number[] | null) =>
  invoke<number[]>("archive_finished", { statuses, ids: ids ?? null });
/** Add tags to downloads; rejects with the reason a tag is invalid. Resolves with the count changed. */
export const addTags = (ids: number[], tags: string[]) => invoke<number>("add_tags", { ids, tags });
/** Remove tags (ignoring case) from downloads. Resolves with the count changed. */
export const removeTags = (ids: number[], tags: string[]) =>
  invoke<number>("remove_tags", { ids, tags });
/**
 * Check a completed download against `checksum` (`sha256:<hex>` or bare hex);
 * the result arrives as the snapshot's `checksum_state`. Rejects if it can't run.
 */
export const verifyChecksum = (id: number, checksum: string) =>
  invoke<void>("verify_checksum", { id, checksum });
/** A download's recent log lines, oldest first (empty for an unknown id). */
export const downloadLog = (id: number) => invoke<LogLine[]>("download_log", { id });
/** What schedules, tags and actions did to a download, oldest first. */
export const downloadHistory = (id: number) => invoke<HistoryEvent[]>("download_history", { id });
/** What a queue window rule did, oldest first. */
export const ruleHistory = (ruleId: number) => invoke<HistoryEvent[]>("rule_history", { ruleId });
/** Remove a download for good; its downloaded file stays on disk (see the engine's `remove`). */
export const remove = (id: number) => invoke<void>("remove", { id });
export const pauseAll = () => invoke<void>("pause_all");
/** Resume every paused download (stopped ones stay stopped). */
export const resumeAll = () => invoke<void>("resume_all");
/** Retry every failed download. */
export const retryFailed = () => invoke<void>("retry_failed");

/** The engine's reading of pasted text: its links (with repeat counts) and skipped lines. Rejects a list over the cap. */
export const previewList = (text: string) => invoke<ParsedList>("preview_list", { text });
/**
 * The path of the file `url` would be saved as in `dir` when it is already
 * there (the engine's loose name and size match), else null. Probes the server.
 */
export const checkExisting = (url: string, dir: string | null) =>
  invoke<string | null>("check_existing", { url, dir });
/** Show that existing file in its folder. By link: the backend matches again, so the webview can't name a path. */
export const revealExisting = (url: string, dir: string | null) =>
  invoke<void>("reveal_existing", { url, dir });
export const defaultDownloadDir = () => invoke<string>("default_download_dir");
export const pickFolder = () => invoke<string | null>("pick_folder");
/** Open a completed download's file. By id: the backend resolves the path, so the webview can't name one. */
export const openDownload = (id: number) => invoke<void>("open_download", { id });
/**
 * Ask the user for the file of a missing download in the OS file picker and relink the record to
 * it. Says whether it relinked (not if they cancelled) and any offer to relink more from the same
 * folder. By id, so the webview names no path.
 */
export const locateDownload = (id: number) => invoke<LocateOutcome>("locate_download", { id });
/** Drop the pending same-folder offer without relinking anything. */
export const dismissSameFolderOffer = () => invoke<void>("dismiss_same_folder_offer");
/**
 * Ask the user for one folder in the OS picker and relink each of the missing downloads `ids`
 * whose same-named, same-sized file is in it. Resolves null if they cancelled.
 */
export const locateFolder = (ids: number[]) =>
  invoke<FolderOutcome | null>("locate_folder", { ids });
/**
 * Search for a missing download's file: its old folder, the other downloads' folders, the default
 * folder. Resolves with the search id; progress and results arrive as find events.
 */
export const findFile = (id: number) => invoke<number>("find_file", { id });
/** Search for every missing download's file in one pass; resolves with the search id. */
export const findFiles = () => invoke<number>("find_files");
/** Stop a search by its id. */
export const cancelFind = (search: number) => invoke<void>("cancel_find", { search });
/** The kept result of every search not yet relinked: the source of truth after a missed event. */
export const findResults = () => invoke<FindResult[]>("find_results");
/**
 * Relink each accepted candidate. The engine takes only a path the last search offered; resolves
 * with how many were relinked.
 */
export const relinkFound = (choices: FoundChoice[]) => invoke<number>("relink_found", { choices });
/** Subscribe to a search's progress, per-download results and end. */
export const onFindEvent = (cb: (event: FindEvent) => void): Promise<UnlistenFn> =>
  listen<FindEvent>("find-event", (e) => cb(e.payload));
/** Show a completed download's file in its folder (by id, like `openDownload`). */
export const revealDownload = (id: number) => invoke<void>("reveal_download", { id });

/** Resume exactly the downloads an update restart paused (the after-update bar's Resume all). */
export const resumePausedForUpdate = () => invoke<void>("resume_paused_for_update");
/** Leave the downloads an update restart paused as they are, and stop offering to resume them. */
export const forgetPausedForUpdate = () => invoke<void>("forget_paused_for_update");

/** Ask the release feed for a newer rdm; null when this is the newest. Rejects if the check fails. */
export const checkForUpdate = () => invoke<UpdateInfo | null>("check_for_update");
/**
 * Download and verify `version`, the update the notice shows, then restart into it now or install
 * it at quit. The backend refuses a version it didn't find or stage.
 */
export const installUpdate = (when: InstallWhen, version: string) =>
  invoke<void>("install_update", { when, version });
/** The version staged to install at quit, if any. */
export const stagedUpdate = () => invoke<string | null>("staged_update");
/** Open the found update's release page. No URL: the backend opens only that page. */
export const openUpdateNotes = () => invoke<void>("open_update_notes");
/** Whether to check automatically (null: not asked yet), this version, and what it updated from. */
export const updatePrefs = () => invoke<UpdatePrefs>("update_prefs");
/** Turn automatic update checks on or off. */
export const setAutoCheck = (on: boolean) => invoke<void>("set_auto_check", { on });
/** Subscribe to an update's download progress. */
export const onUpdateProgress = (cb: (progress: UpdateProgress) => void): Promise<UnlistenFn> =>
  listen<UpdateProgress>("update-progress", (e) => cb(e.payload));

/** Record an error the window caught in the app log (target `rdm::frontend`). */
export const logFrontendError = (
  message: string,
  stack: string | null,
  componentStack: string | null,
  source: string,
) => invoke<void>("log_frontend_error", { message, stack, componentStack, source });
/** The folder holding rdm's log files, or null without a file log. */
export const logsFolder = () => invoke<string | null>("logs_folder");
/** Open the logs folder in the OS file manager. */
export const openLogsFolder = () => invoke<void>("open_logs_folder");
/** Open GitHub's bug report form in the browser. */
export const openBugReport = () => invoke<void>("open_bug_report");
/** The redacted troubleshooting report for a bug report; `extra` adds a caught error. */
export const diagnostics = (extra?: string | null) =>
  invoke<string>("diagnostics", { extra: extra ?? null });
/** Save `text` to a file picked in a save dialog; resolves with its path, or null if cancelled. */
export const saveTextFile = (defaultName: string, text: string) =>
  invoke<string | null>("save_text_file", { defaultName, text });

/** Subscribe to live snapshots pushed from the backend's watch channel. */
export const onDownloadsChanged = (
  cb: (snapshots: DownloadSnapshot[]) => void,
): Promise<UnlistenFn> => listen<DownloadSnapshot[]>("downloads-changed", (e) => cb(e.payload));

/** The saved schedule rules. */
export const getSchedule = () => invoke<Schedule>("get_schedule");

/** Replace the whole rule set and save it. Rejects with why a rule was refused. */
export const setSchedule = (schedule: Schedule) => invoke<void>("set_schedule", { schedule });

/** Whether the queue window is open now, and when that next changes. */
export const getQueueState = () => invoke<QueueState>("get_queue_state");

/** Subscribe to the queue opening or closing, or its rules changing. */
export const onQueueStateChanged = (cb: (state: QueueState) => void): Promise<UnlistenFn> =>
  listen<QueueState>("queue-state-changed", (e) => cb(e.payload));

/** The command and power action run when downloads finish. */
export const getPostActions = () => invoke<PostActions>("get_post_actions");

/** Replace both actions and save them. Rejects with why one was refused. */
export const setPostActions = (actions: PostActions) =>
  invoke<void>("set_post_actions", { actions });

/** Where the power action stands: what it waits for, or its countdown. */
export const getActionsStatus = () => invoke<ActionsStatus>("get_actions_status");

/** Cancel a power action that is counting down. */
export const cancelPower = () => invoke<void>("cancel_power");

/** Subscribe to the power action's status changing (its countdown ticks each second). */
export const onActionsStatusChanged = (cb: (status: ActionsStatus) => void): Promise<UnlistenFn> =>
  listen<ActionsStatus>("actions-status-changed", (e) => cb(e.payload));
