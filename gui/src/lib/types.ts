// Mirrors the engine's `DownloadSnapshot` (see src/engine/manager.rs). Field
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
}

/** Mirrors the engine's manager-wide `Settings` (see src/engine/store.rs). */
export interface Settings {
  max_concurrent: number;
  /** Aggregate speed cap in bytes/s; null is unlimited. */
  global_limit: number | null;
  max_connections: number;
}
