// Test-only factory for engine snapshots; imported by the colocated *.test.ts files.

import type { DownloadSnapshot } from "./types";

/** A queued 0-byte download with `overrides` applied. */
export function snap(overrides: Partial<DownloadSnapshot> = {}): DownloadSnapshot {
  const id = overrides.id ?? 1;
  return {
    id,
    url: `http://example.com/file-${id}.iso`,
    filename: `file-${id}.iso`,
    target: `/downloads/file-${id}.iso`,
    final_path: null,
    status: "queued",
    downloaded: 0,
    total: null,
    bytes_per_sec: 0,
    eta_secs: null,
    error: null,
    added_at: null,
    finished_at: null,
    limit: null,
    connections: 0,
    partial_kept: false,
    checksum: null,
    checksum_state: "none",
    segments: [],
    lanes: [],
    peak_lanes: 0,
    active_ms: null,
    archived: false,
    tags: [],
    paused_for_update: false,
    file_missing: false,
    ...overrides,
  };
}
