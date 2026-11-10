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

describe("the updater", () => {
  beforeEach(() => invoke.mockReset().mockResolvedValue(undefined));

  // The release page is the backend's to choose: the webview never passes a URL.
  it("sends only what each command needs", async () => {
    await api.checkForUpdate();
    await api.installUpdate("on-quit", "0.3.1");
    await api.openUpdateNotes();
    await api.setAutoCheck(true);
    await api.resumePausedForUpdate();
    await api.forgetPausedForUpdate();
    expect(invoke.mock.calls).toEqual([
      ["check_for_update"],
      ["install_update", { when: "on-quit", version: "0.3.1" }],
      ["open_update_notes"],
      ["set_auto_check", { on: true }],
      ["resume_paused_for_update"],
      ["forget_paused_for_update"],
    ]);
  });
});

describe("troubleshooting", () => {
  beforeEach(() => invoke.mockReset().mockResolvedValue(null));

  it("names the commands and their arguments as the backend does", async () => {
    await api.logFrontendError("TypeError: x", "at f", "at App", "render");
    await api.logsFolder();
    await api.openLogsFolder();
    await api.openBugReport();
    await api.diagnostics();
    await api.diagnostics("Window error: x");
    await api.saveTextFile("a.iso.log", "09:00:00  Started");
    expect(invoke.mock.calls).toEqual([
      [
        "log_frontend_error",
        { message: "TypeError: x", stack: "at f", componentStack: "at App", source: "render" },
      ],
      ["logs_folder"],
      ["open_logs_folder"],
      ["open_bug_report"],
      ["diagnostics", { extra: null }],
      ["diagnostics", { extra: "Window error: x" }],
      ["save_text_file", { defaultName: "a.iso.log", text: "09:00:00  Started" }],
    ]);
  });
});
