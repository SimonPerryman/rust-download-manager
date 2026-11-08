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
