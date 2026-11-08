// Typed wrappers over the Tauri command surface. Every function here is a
// one-line pass-through to a backend command, which is itself a thin wrapper
// over the engine's `Manager`. No download logic lives in the frontend.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SettingsPatch } from "./settings";
import type { DownloadSnapshot, LogLine, Settings, Status } from "./types";

/** One-shot snapshot of every download, in display order. */
export const list = () => invoke<DownloadSnapshot[]>("list");

/**
 * Add a download; `dir`/`checksum`/`limit` optional (`limit` is a rate like
 * `"500K"`). Returns the new id, or rejects with a parse/validation error.
 */
export const addDownload = (
  url: string,
  dir?: string | null,
  checksum?: string | null,
  limit?: string | null,
) =>
  invoke<number>("add_download", {
    url,
    dir: dir ?? null,
    checksum: checksum ?? null,
    limit: limit ?? null,
  });

/** The engine's manager-wide settings. */
export const getSettings = () => invoke<Settings>("get_settings");

/**
 * Apply some manager-wide settings; omitted fields are left unchanged
 * (`globalLimit` is a rate string, blank = unlimited). Resolves with the
 * settings as the engine clamped them.
 */
export const setSettings = (patch: SettingsPatch) =>
  invoke<Settings>("set_settings", {
    maxConcurrent: patch.maxConcurrent ?? null,
    globalLimit: patch.globalLimit ?? null,
    maxConnections: patch.maxConnections ?? null,
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
/** Remove finished downloads in `statuses` (only `ids`, if given). Resolves with the count. */
export const clear = (statuses: Status[], ids?: number[] | null) =>
  invoke<number>("clear", { statuses, ids: ids ?? null });
/**
 * Check a completed download against `checksum` (`sha256:<hex>` or bare hex);
 * the result arrives as the snapshot's `checksum_state`. Rejects if it can't run.
 */
export const verifyChecksum = (id: number, checksum: string) =>
  invoke<void>("verify_checksum", { id, checksum });
/** A download's recent log lines, oldest first (empty for an unknown id). */
export const downloadLog = (id: number) => invoke<LogLine[]>("download_log", { id });
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

/** Subscribe to live snapshots pushed from the backend's watch channel. */
export const onDownloadsChanged = (
  cb: (snapshots: DownloadSnapshot[]) => void,
): Promise<UnlistenFn> => listen<DownloadSnapshot[]>("downloads-changed", (e) => cb(e.payload));
