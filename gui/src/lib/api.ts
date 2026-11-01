// Typed wrappers over the Tauri command surface. Every function here is a
// one-line pass-through to a backend command, which is itself a thin wrapper
// over the engine's `Manager`. No download logic lives in the frontend.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DownloadSnapshot } from "./types";

/** One-shot snapshot of every download, in display order. */
export const list = () => invoke<DownloadSnapshot[]>("list");

/** Add a download; `dir`/`checksum` optional. Returns the new id (or rejects). */
export const addDownload = (url: string, dir?: string | null, checksum?: string | null) =>
  invoke<number>("add_download", {
    url,
    dir: dir ?? null,
    checksum: checksum ?? null,
  });

export const pause = (id: number) => invoke<void>("pause", { id });
export const resume = (id: number) => invoke<void>("resume", { id });
export const cancel = (id: number) => invoke<void>("cancel", { id });
export const remove = (id: number) => invoke<void>("remove", { id });
export const pauseAll = () => invoke<void>("pause_all");

export const defaultDownloadDir = () => invoke<string>("default_download_dir");
export const pickFolder = () => invoke<string | null>("pick_folder");
export const openPath = (path: string) => invoke<void>("open_path", { path });
export const revealPath = (path: string) => invoke<void>("reveal_path", { path });

/** Subscribe to live snapshots pushed from the backend's watch channel. */
export const onDownloadsChanged = (
  cb: (snapshots: DownloadSnapshot[]) => void,
): Promise<UnlistenFn> => listen<DownloadSnapshot[]>("downloads-changed", (e) => cb(e.payload));
