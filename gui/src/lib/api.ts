// Typed wrappers over the Tauri command surface. Every function here is a
// one-line pass-through to a backend command, which is itself a thin wrapper
// over the engine's `Manager`. No download logic lives in the frontend.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SettingsPatch } from "./settings";
import type {
  DownloadSnapshot,
  InstallWhen,
  LogLine,
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
) =>
  invoke<number>("add_download", {
    url,
    dir: dir ?? null,
    checksum: checksum ?? null,
    limit: limit ?? null,
    tags: tags ?? null,
  });

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
/** Remove a download for good; its downloaded file stays on disk (see the engine's `remove`). */
export const remove = (id: number) => invoke<void>("remove", { id });
export const pauseAll = () => invoke<void>("pause_all");
/** Resume every paused download (stopped ones stay stopped). */
export const resumeAll = () => invoke<void>("resume_all");
/** Retry every failed download. */
export const retryFailed = () => invoke<void>("retry_failed");

export const defaultDownloadDir = () => invoke<string>("default_download_dir");
export const pickFolder = () => invoke<string | null>("pick_folder");
/** Open a completed download's file. By id: the backend resolves the path, so the webview can't name one. */
export const openDownload = (id: number) => invoke<void>("open_download", { id });
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
