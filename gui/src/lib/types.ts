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

/** Mirrors the engine's manager-wide `Settings` (see src/engine/store.rs). */
export interface Settings {
  max_concurrent: number;
  /** Aggregate speed cap in bytes/s; null is unlimited. */
  global_limit: number | null;
  max_connections: number;
}
