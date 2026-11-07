import { describe, expect, it } from "vitest";
import { connectionsLabel } from "./format";
import type { DownloadSnapshot, Status } from "./types";

function snapshot(status: Status, connections: number): DownloadSnapshot {
  return {
    id: 1,
    url: "http://example.com/file.iso",
    filename: "file.iso",
    target: "/downloads/file.iso",
    final_path: null,
    status,
    downloaded: 0,
    total: null,
    bytes_per_sec: 0,
    eta_secs: null,
    error: null,
    added_at: null,
    finished_at: null,
    limit: null,
    connections,
    partial_kept: false,
    checksum: null,
    checksum_state: "none",
    segments: [],
  };
}

describe("connectionsLabel", () => {
  it("names the live count while downloading over several connections", () => {
    expect(connectionsLabel(snapshot("downloading", 2))).toBe("2 connections");
    expect(connectionsLabel(snapshot("downloading", 8))).toBe("8 connections");
  });

  it("stays quiet for a single connection", () => {
    expect(connectionsLabel(snapshot("downloading", 1))).toBeNull();
    expect(connectionsLabel(snapshot("downloading", 0))).toBeNull();
  });

  it("stays quiet unless downloading", () => {
    for (const status of ["queued", "paused", "completed", "failed", "cancelled"] as const) {
      expect(connectionsLabel(snapshot(status, 4))).toBeNull();
    }
  });
});
