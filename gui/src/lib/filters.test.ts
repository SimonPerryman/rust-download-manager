import { describe, expect, it } from "vitest";
import {
  filterKeyOf,
  matchesFilter,
  matchesQuery,
  sameFilter,
  statusCounts,
  STATUS_FILTERS,
  type StatusFilter,
} from "./filters";
import { snap } from "./testSnapshot";
import type { Status } from "./types";

const STATUSES: Status[] = ["queued", "downloading", "paused", "completed", "failed", "cancelled"];

describe("filterKeyOf", () => {
  it("maps each engine status to exactly one filter key", () => {
    expect(STATUSES.map(filterKeyOf)).toEqual([
      "waiting",
      "downloading",
      "paused",
      "done",
      "failed",
      "stopped",
    ]);
  });

  it("covers every status filter but All, once each", () => {
    const keys = STATUS_FILTERS.map((f) => f.key).filter((k) => k !== "all");
    expect(new Set(STATUSES.map(filterKeyOf))).toEqual(new Set(keys));
    expect(STATUS_FILTERS[0]).toEqual({ key: "all", label: "All" });
  });
});

describe("matchesFilter", () => {
  it("All matches every status", () => {
    for (const status of STATUSES) {
      expect(matchesFilter(snap({ status }), { kind: "status", status: "all" })).toBe(true);
    }
  });

  it("a status filter matches only its statuses", () => {
    const d = snap({ status: "cancelled" });
    expect(matchesFilter(d, { kind: "status", status: "stopped" })).toBe(true);
    expect(matchesFilter(d, { kind: "status", status: "done" })).toBe(false);
  });

  it("a folder filter matches the target's parent dir", () => {
    const d = snap({ target: "C:\\Users\\a\\Downloads\\x.iso" });
    expect(matchesFilter(d, { kind: "folder", dir: "C:\\Users\\a\\Downloads" })).toBe(true);
    expect(matchesFilter(d, { kind: "folder", dir: "C:\\Users\\a" })).toBe(false);
  });
});

describe("matchesQuery", () => {
  const d = snap({ filename: "Ubuntu-24.04.iso", url: "https://Releases.Ubuntu.com/x" });

  it("hits the filename or the host, case-insensitively", () => {
    expect(matchesQuery(d, "ubuntu-24")).toBe(true);
    expect(matchesQuery(d, "RELEASES.ubuntu")).toBe(true);
    expect(matchesQuery(d, "fedora")).toBe(false);
  });

  it("a blank query matches everything, and the query is trimmed", () => {
    expect(matchesQuery(d, "")).toBe(true);
    expect(matchesQuery(d, "   ")).toBe(true);
    expect(matchesQuery(d, "  iso ")).toBe(true);
  });
});

describe("statusCounts", () => {
  it("is all zeros for no downloads", () => {
    const counts = statusCounts([]);
    for (const { key } of STATUS_FILTERS) expect(counts[key]).toBe(0);
  });

  it("counts each status, and the per-status counts sum to All", () => {
    const ds = [
      ...STATUSES.map((status, i) => snap({ id: i + 1, status })),
      snap({ id: 10, status: "completed" }),
    ];
    const counts = statusCounts(ds);
    expect(counts.all).toBe(7);
    expect(counts.done).toBe(2);
    expect(counts.waiting).toBe(1);
    const keys: StatusFilter[] = STATUS_FILTERS.map((f) => f.key).filter((k) => k !== "all");
    expect(keys.reduce((sum, k) => sum + counts[k], 0)).toBe(counts.all);
  });
});

describe("sameFilter", () => {
  it("compares kind and value", () => {
    expect(sameFilter({ kind: "status", status: "all" }, { kind: "status", status: "all" })).toBe(
      true,
    );
    expect(sameFilter({ kind: "status", status: "all" }, { kind: "status", status: "done" })).toBe(
      false,
    );
    expect(sameFilter({ kind: "folder", dir: "/a" }, { kind: "folder", dir: "/a" })).toBe(true);
    expect(sameFilter({ kind: "folder", dir: "/a" }, { kind: "folder", dir: "/b" })).toBe(false);
    expect(sameFilter({ kind: "folder", dir: "all" }, { kind: "status", status: "all" })).toBe(
      false,
    );
  });
});
