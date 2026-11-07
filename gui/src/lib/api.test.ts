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
