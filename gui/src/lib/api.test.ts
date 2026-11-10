import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const api = await import("./api");

describe("open and reveal", () => {
  beforeEach(() => invoke.mockReset().mockResolvedValue(undefined));

  // The backend resolves the path from the id, so the webview never names a path.
  it("send only the download's id", async () => {
    await api.openDownload(7);
    await api.revealDownload(8);
    expect(invoke.mock.calls).toEqual([
      ["open_download", { id: 7 }],
      ["reveal_download", { id: 8 }],
    ]);
  });
});

describe("downloadLog", () => {
  beforeEach(() => invoke.mockReset().mockResolvedValue([]));

  it("asks for one download's lines by id", async () => {
    await api.downloadLog(5);
    expect(invoke.mock.calls).toEqual([["download_log", { id: 5 }]]);
  });
});

describe("archive and tags", () => {
  beforeEach(() => invoke.mockReset().mockResolvedValue(0));

  it("send ids and tags as the backend commands name them", async () => {
    await api.archive([1, 2]);
    await api.unarchive([2]);
    await api.archiveFinished(["completed", "cancelled"], [1]);
    await api.archiveFinished(["completed"]);
    await api.addTags([3], ["work"]);
    await api.removeTags([3], ["work"]);
    await api.addDownload("http://x/a", null, null, null, ["iso"]);
    expect(invoke.mock.calls).toEqual([
      ["archive", { ids: [1, 2] }],
      ["unarchive", { ids: [2] }],
      ["archive_finished", { statuses: ["completed", "cancelled"], ids: [1] }],
      ["archive_finished", { statuses: ["completed"], ids: null }],
      ["add_tags", { ids: [3], tags: ["work"] }],
      ["remove_tags", { ids: [3], tags: ["work"] }],
      [
        "add_download",
        { url: "http://x/a", dir: null, checksum: null, limit: null, tags: ["iso"] },
      ],
    ]);
  });
});
