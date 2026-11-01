// Mirrors the engine's `DownloadSnapshot` (see src/engine/manager.rs). Field
// names and the `status` set are the frozen contract the backend serializes.
// There is intentionally no "retrying" status: the engine retries internally.

export type Status = "queued" | "downloading" | "paused" | "completed" | "failed" | "cancelled";

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
}
