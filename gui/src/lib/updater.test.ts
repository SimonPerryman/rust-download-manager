import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { snap } from "./testSnapshot";
import type { UpdateInfo, UpdatePrefs, UpdateProgress } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const {
  CHECK_EVERY_MS,
  CHECK_TICK_MS,
  NOTICE_ACTION_LABELS,
  UpdateController,
  afterMiddot,
  checkResultLine,
  noticeView,
  pausedBarCount,
  pausedForUpdateCount,
  pausedForUpdateLine,
  progressText,
  updatedHeading,
  willPauseCount,
  willPauseLine,
} = await import("./updater");
type Notice = import("./updater").Notice;

const info = (over: Partial<UpdateInfo> = {}): UpdateInfo => ({
  version: "0.3.1",
  currentVersion: "0.3.0",
  notesUrl: "https://example.invalid/releases/tag/v0.3.1",
  canInstall: true,
  ...over,
});

/** A fake backend: answers set per test, every call recorded. */
function fakeBackend(prefs: Partial<UpdatePrefs> = {}) {
  let progress: ((p: UpdateProgress) => void) | null = null;
  const backend = {
    updatePrefs: vi.fn(async () => ({
      autoCheck: null,
      version: "0.3.0",
      updatedFrom: null,
      ...prefs,
    })),
    setAutoCheck: vi.fn(async (_on: boolean) => {}),
    checkForUpdate: vi.fn(async (): Promise<UpdateInfo | null> => null),
    installUpdate: vi.fn(async (_when: "now" | "on-quit", _version: string) => {}),
    stagedUpdate: vi.fn(async (): Promise<string | null> => null),
    onUpdateProgress: vi.fn(async (cb: (p: UpdateProgress) => void) => {
      progress = cb;
      return () => {
        progress = null;
      };
    }),
    resumePausedForUpdate: vi.fn(async () => {}),
    forgetPausedForUpdate: vi.fn(async () => {}),
  };
  return { backend, emitProgress: (p: UpdateProgress) => progress?.(p) };
}

const ctx = (willPause = 0, staged: string | null = null) => ({ willPause, staged });

/** Labels of the notice's buttons and links, left to right. */
const actions = (n: Notice, willPause = 0) =>
  noticeView(n, ctx(willPause))?.actions.map((a) => NOTICE_ACTION_LABELS[a]) ?? [];

const pausedRow = () => [snap({ id: 1, status: "paused", paused_for_update: true })];

describe("the first-run question", () => {
  it("shows while unanswered, and nothing is checked before the answer", async () => {
    const { backend } = fakeBackend();
    const c = new UpdateController(backend);
    await c.start();
    expect(c.getState().prefs?.autoCheck).toBeNull();
    expect(backend.checkForUpdate).not.toHaveBeenCalled();
  });

  it("Yes saves the answer, then checks at once", async () => {
    const { backend } = fakeBackend();
    backend.checkForUpdate.mockResolvedValue(info());
    const c = new UpdateController(backend);
    await c.start();
    await c.answer(true);
    expect(backend.setAutoCheck).toHaveBeenCalledWith(true);
    expect(backend.setAutoCheck.mock.invocationCallOrder[0]).toBeLessThan(
      backend.checkForUpdate.mock.invocationCallOrder[0],
    );
    expect(c.getState().prefs?.autoCheck).toBe(true);
    expect(c.getState().notice).toEqual({ kind: "available", info: info() });
    expect(c.getState().checksDone).toBe(1);
    c.stop();
  });

  it("No saves the answer and checks nothing", async () => {
    const { backend } = fakeBackend();
    const c = new UpdateController(backend);
    await c.start();
    await c.answer(false);
    expect(backend.setAutoCheck).toHaveBeenCalledWith(false);
    expect(c.getState().prefs?.autoCheck).toBe(false);
    expect(backend.checkForUpdate).not.toHaveBeenCalled();
  });

  it("asks again, and says why, when the answer can't be saved", async () => {
    const { backend } = fakeBackend();
    backend.setAutoCheck.mockRejectedValue("Couldn't save the setting: disk full");
    const c = new UpdateController(backend);
    await c.start();
    await expect(c.answer(true)).rejects.toBe("Couldn't save the setting: disk full");
    expect(c.getState().prefs?.autoCheck).toBeNull();
    expect(backend.checkForUpdate).not.toHaveBeenCalled();
  });

  it("isn't asked again once answered", async () => {
    const { backend } = fakeBackend({ autoCheck: false });
    const c = new UpdateController(backend);
    await c.start();
    expect(c.getState().prefs?.autoCheck).toBe(false);
  });
});

describe("automatic checks", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("run on start and then daily while on", async () => {
    const { backend } = fakeBackend({ autoCheck: true });
    const c = new UpdateController(backend);
    await c.start();
    expect(backend.checkForUpdate).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS - CHECK_TICK_MS);
    expect(backend.checkForUpdate).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(CHECK_TICK_MS);
    expect(backend.checkForUpdate).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS);
    expect(backend.checkForUpdate).toHaveBeenCalledTimes(3);
    c.stop();
  });

  // Timers stall while the machine sleeps; the wall clock doesn't.
  it("catch up within the hour when the machine slept past the due time", async () => {
    const { backend } = fakeBackend({ autoCheck: true });
    const c = new UpdateController(backend);
    await c.start();
    vi.setSystemTime(Date.now() + 2 * CHECK_EVERY_MS);
    await vi.advanceTimersByTimeAsync(CHECK_TICK_MS);
    expect(backend.checkForUpdate).toHaveBeenCalledTimes(2);
    c.stop();
  });

  it("don't run while off", async () => {
    const { backend } = fakeBackend({ autoCheck: false });
    const c = new UpdateController(backend);
    await c.start();
    await vi.advanceTimersByTimeAsync(3 * CHECK_EVERY_MS);
    expect(backend.checkForUpdate).not.toHaveBeenCalled();
  });

  it("stop when turned off in Settings, and start again when turned on", async () => {
    const { backend } = fakeBackend({ autoCheck: true });
    const c = new UpdateController(backend);
    await c.start();
    await c.setAutoCheck(false);
    expect(backend.setAutoCheck).toHaveBeenLastCalledWith(false);
    await vi.advanceTimersByTimeAsync(2 * CHECK_EVERY_MS);
    expect(backend.checkForUpdate).toHaveBeenCalledTimes(1);

    await c.setAutoCheck(true);
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS);
    expect(backend.checkForUpdate).toHaveBeenCalledTimes(2);
    c.stop();
  });

  it("turned off while one runs, drop its answer", async () => {
    const { backend } = fakeBackend({ autoCheck: true });
    let answer: (v: UpdateInfo | null) => void = () => {};
    backend.checkForUpdate.mockImplementation(() => new Promise((r) => (answer = r)));
    const c = new UpdateController(backend);
    const started = c.start();
    await vi.waitFor(() => expect(backend.checkForUpdate).toHaveBeenCalled());
    await c.setAutoCheck(false);
    answer(info());
    await started;
    expect(c.getState().notice).toEqual({ kind: "hidden" });
  });

  it("show nothing when they fail", async () => {
    const { backend } = fakeBackend({ autoCheck: true });
    backend.checkForUpdate.mockRejectedValue("offline");
    const c = new UpdateController(backend);
    await c.start();
    expect(c.getState().notice).toEqual({ kind: "hidden" });
    expect(c.getState().check).toEqual({ kind: "idle" });
    c.stop();
  });

  it("are dropped once the app has stopped", async () => {
    const { backend } = fakeBackend({ autoCheck: true });
    const c = new UpdateController(backend);
    await c.start();
    c.stop();
    await vi.advanceTimersByTimeAsync(2 * CHECK_EVERY_MS);
    expect(backend.checkForUpdate).toHaveBeenCalledTimes(1);
  });
});

describe("Check for updates", () => {
  it("runs with automatic checks off and says you're up to date", async () => {
    const { backend } = fakeBackend({ autoCheck: false });
    const c = new UpdateController(backend);
    await c.start();
    const running = c.checkNow();
    expect(checkResultLine(c.getState().check)).toBe("Checking…");
    await running;
    expect(backend.checkForUpdate).toHaveBeenCalledTimes(1);
    expect(checkResultLine(c.getState().check)).toBe("You're up to date.");
    expect(c.getState().notice).toEqual({ kind: "hidden" });
  });

  it("reports an error", async () => {
    const { backend } = fakeBackend({ autoCheck: false });
    backend.checkForUpdate.mockRejectedValue("connection refused");
    const c = new UpdateController(backend);
    await c.start();
    await c.checkNow();
    expect(checkResultLine(c.getState().check)).toBe(
      "Couldn't check for updates: connection refused",
    );
  });

  it("points at the notice when it finds an update", async () => {
    const { backend } = fakeBackend({ autoCheck: false });
    backend.checkForUpdate.mockResolvedValue(info());
    const c = new UpdateController(backend);
    await c.start();
    await c.checkNow();
    expect(checkResultLine(c.getState().check)).toBe(
      "rdm 0.3.1 is available (see the bar above the list).",
    );
    expect(c.getState().notice.kind).toBe("available");
  });
});

describe("the update notice", () => {
  async function offered(over: Partial<UpdateInfo> = {}) {
    const fake = fakeBackend({ autoCheck: true });
    fake.backend.checkForUpdate.mockResolvedValue(info(over));
    const c = new UpdateController(fake.backend);
    await c.start();
    return { ...fake, c };
  }

  it("shows the version, What's new, Later, Install when I quit and Restart now", async () => {
    const { c } = await offered();
    const view = noticeView(c.getState().notice, ctx())!;
    expect(view.title).toBe("rdm 0.3.1 is available");
    expect(view.notes).toBe(true);
    expect(view.lines).toEqual([]);
    expect(actions(c.getState().notice)).toEqual(["Later", "Install when I quit", "Restart now"]);
    c.stop();
  });

  it("names the downloads Restart now will pause", () => {
    const n: Notice = { kind: "available", info: info() };
    expect(noticeView(n, ctx(2))!.lines).toEqual([
      "2 downloads will pause; you can resume them after the restart.",
    ]);
    expect(noticeView(n, ctx(1))!.lines).toEqual([
      "1 download will pause; you can resume it after the restart.",
    ]);
    const ds = [
      snap({ id: 1, status: "downloading" }),
      snap({ id: 2, status: "queued" }),
      snap({ id: 3, status: "paused" }),
      snap({ id: 4, status: "completed" }),
    ];
    expect(willPauseCount(ds)).toBe(2);
    expect(willPauseLine(0)).toBeNull();
  });

  it("Later hides it until the next check", async () => {
    vi.useFakeTimers();
    const { c } = await offered();
    c.later();
    expect(noticeView(c.getState().notice, ctx())).toBeNull();
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS);
    expect(c.getState().notice.kind).toBe("available");
    c.stop();
    vi.useRealTimers();
  });

  it("Restart now shows the download's progress, then restarts", async () => {
    const { c, backend, emitProgress } = await offered();
    let finish = () => {};
    backend.installUpdate.mockImplementation(() => new Promise<void>((r) => (finish = r)));
    const installing = c.install("now");
    expect(backend.installUpdate).toHaveBeenCalledWith("now", "0.3.1");
    expect(noticeView(c.getState().notice, ctx())!.title).toBe("Downloading rdm 0.3.1…");
    emitProgress({ downloaded: 4.1 * 1024 * 1024, total: 8.9 * 1024 * 1024 });
    const view = noticeView(c.getState().notice, ctx())!;
    expect(view.downloading).toBe(true);
    expect(progressText(view.progress)).toBe("4.10 MB of 8.90 MB");
    expect(view.actions).toEqual([]);
    finish();
    await installing;
    expect(c.getState().notice).toEqual({ kind: "restarting", version: "0.3.1" });
    c.stop();
  });

  it("Install when I quit stages it, and Restart now instead installs it now", async () => {
    const { c, backend } = await offered();
    await c.install("on-quit");
    expect(backend.installUpdate).toHaveBeenLastCalledWith("on-quit", "0.3.1");
    const staged = c.getState().notice;
    expect(noticeView(staged, ctx(3))!.title).toBe("rdm 0.3.1 will install when you quit");
    // Restart now instead pauses downloads too: it says so.
    expect(noticeView(staged, ctx(3))!.lines).toEqual([
      "3 downloads will pause; you can resume them after the restart.",
    ]);
    expect(actions(staged)).toEqual(["Restart now instead"]);

    // A later check that finds the same version keeps the staged notice.
    await c.checkNow();
    expect(c.getState().notice).toEqual(staged);
    expect(checkResultLine(c.getState().check)).toBe("rdm 0.3.1 will install when you quit.");

    await c.install("now");
    expect(backend.installUpdate).toHaveBeenLastCalledWith("now", "0.3.1");
    expect(c.getState().notice.kind).toBe("restarting");
    c.stop();
  });

  it("an update staged before a reload is shown as staged", async () => {
    const { backend } = fakeBackend({ autoCheck: false });
    backend.stagedUpdate.mockResolvedValue("0.3.1");
    const c = new UpdateController(backend);
    await c.start();
    expect(c.getState().notice).toEqual({ kind: "staged", version: "0.3.1" });
  });

  it("an error shows its message; Try again repeats the action and Dismiss hides it", async () => {
    const { c, backend } = await offered();
    backend.installUpdate.mockRejectedValueOnce(
      "The update's signature didn't verify, so it wasn't installed.",
    );
    await c.install("on-quit");
    const failed = c.getState().notice;
    const view = noticeView(failed, ctx(2))!;
    expect(view.title).toBe("Couldn't install rdm 0.3.1");
    expect(view.detail).toBe("the update's signature didn't verify, so it wasn't installed.");
    // Trying Install when I quit again pauses nothing.
    expect(view.lines).toEqual([]);
    expect(actions(failed)).toEqual(["Dismiss", "Try again"]);

    await c.retry();
    expect(backend.installUpdate).toHaveBeenLastCalledWith("on-quit", "0.3.1");
    expect(c.getState().notice.kind).toBe("staged");

    backend.installUpdate.mockRejectedValueOnce("disk full");
    await c.install("now");
    // Try again would restart now: it warns about the downloads it would pause.
    expect(noticeView(c.getState().notice, ctx(1))!.lines).toEqual([
      "1 download will pause; you can resume it after the restart.",
    ]);
    c.dismiss();
    // Back to the update still waiting for the quit; the app carries on.
    expect(c.getState().notice.kind).toBe("staged");
    await c.checkNow();
    expect(c.getState().check.kind).toBe("staged");
    c.stop();
  });

  it("a later check leaves an install error showing, unless it finds another version", async () => {
    vi.useFakeTimers();
    const { c, backend } = await offered();
    backend.installUpdate.mockRejectedValueOnce("disk full");
    await c.install("now");
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS);
    expect(backend.checkForUpdate).toHaveBeenCalledTimes(2);
    expect(c.getState().notice.kind).toBe("failed");

    backend.checkForUpdate.mockResolvedValue(info({ version: "0.3.2" }));
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS);
    expect(c.getState().notice).toEqual({ kind: "available", info: info({ version: "0.3.2" }) });
    c.stop();
    vi.useRealTimers();
  });

  it("a newer version found while one is staged says the staged one still waits", async () => {
    const { c, backend } = await offered();
    await c.install("on-quit");
    backend.checkForUpdate.mockResolvedValue(info({ version: "0.3.2" }));
    await c.checkNow();
    const view = noticeView(c.getState().notice, ctx(0, c.getState().staged))!;
    expect(view.title).toBe("rdm 0.3.2 is available");
    expect(view.lines).toEqual([
      "rdm 0.3.1 will install when you quit, unless you install rdm 0.3.2.",
    ]);
    // Installing the newer one at quit takes the staged one's place.
    await c.install("on-quit");
    expect(backend.installUpdate).toHaveBeenLastCalledWith("on-quit", "0.3.2");
    expect(c.getState().staged).toBe("0.3.2");
    c.stop();
  });

  it("Restart now instead installs the staged version the notice shows, not a newer one found", async () => {
    const { c, backend } = await offered();
    await c.install("on-quit");
    backend.checkForUpdate.mockResolvedValue(info({ version: "0.3.2" }));
    await c.checkNow();
    backend.installUpdate.mockRejectedValueOnce("disk full");
    await c.install("now");
    expect(backend.installUpdate).toHaveBeenLastCalledWith("now", "0.3.2");
    c.dismiss();
    const notice = c.getState().notice;
    expect(noticeView(notice, ctx(0, c.getState().staged))!.title).toBe(
      "rdm 0.3.1 will install when you quit",
    );

    await c.install("now");
    expect(backend.installUpdate).toHaveBeenLastCalledWith("now", "0.3.1");
    expect(c.getState().notice).toEqual({ kind: "restarting", version: "0.3.1" });
    c.stop();
  });

  it("an install error reads on from the middot, as the other states do", () => {
    expect(afterMiddot("Couldn't download the update: timed out")).toBe(
      "couldn't download the update: timed out",
    );
    expect(afterMiddot("HTTP 404")).toBe("HTTP 404");
  });

  it("a .deb install offers Download (the release page) instead of installing", async () => {
    const { c } = await offered({ canInstall: false });
    const view = noticeView(c.getState().notice, ctx(2))!;
    expect(view.notes).toBe(true);
    expect(view.detail).toBe("get the new .deb from the release page.");
    expect(view.lines).toEqual([]);
    expect(actions(c.getState().notice)).toEqual(["Later", "Download"]);
    c.stop();
  });
});

describe("the paused-for-the-update bar", () => {
  it("counts the downloads the update paused", () => {
    const ds = [
      snap({ id: 1, status: "paused", paused_for_update: true }),
      snap({ id: 2, status: "paused" }),
      snap({ id: 3, status: "paused", paused_for_update: true }),
    ];
    expect(pausedForUpdateCount(ds)).toBe(2);
    expect(pausedForUpdateLine(2)).toBe("2 downloads were paused for the update.");
    expect(pausedForUpdateLine(1)).toBe("1 download was paused for the update.");
    // Resumed by hand (or Resume all / dismiss cleared the marks): no bar.
    expect(pausedForUpdateCount([snap({ id: 1, status: "downloading" })])).toBe(0);
  });

  it("Resume all resumes those through the backend and hides it at once", async () => {
    const { backend } = fakeBackend({ autoCheck: false });
    const c = new UpdateController(backend);
    await c.start();
    expect(pausedBarCount(pausedRow(), c.getState())).toBe(1);
    await c.resumePaused();
    expect(backend.resumePausedForUpdate).toHaveBeenCalledTimes(1);
    expect(backend.forgetPausedForUpdate).not.toHaveBeenCalled();
    expect(pausedBarCount(pausedRow(), c.getState())).toBe(0);
  });

  it("dismiss leaves them paused through the backend and hides it at once", async () => {
    const { backend } = fakeBackend({ autoCheck: false });
    const c = new UpdateController(backend);
    await c.start();
    await c.forgetPaused();
    expect(backend.forgetPausedForUpdate).toHaveBeenCalledTimes(1);
    expect(backend.resumePausedForUpdate).not.toHaveBeenCalled();
    expect(pausedBarCount(pausedRow(), c.getState())).toBe(0);
  });

  it("comes back if Resume all fails", async () => {
    const { backend } = fakeBackend({ autoCheck: false });
    backend.resumePausedForUpdate.mockRejectedValue("closed");
    const c = new UpdateController(backend);
    await c.start();
    await expect(c.resumePaused()).rejects.toBe("closed");
    expect(pausedBarCount(pausedRow(), c.getState())).toBe(1);
  });

  it("stays hidden while this app is restarting into an update", async () => {
    const { backend } = fakeBackend({ autoCheck: true });
    backend.checkForUpdate.mockResolvedValue(info());
    const c = new UpdateController(backend);
    await c.start();
    await c.install("now");
    expect(c.getState().notice.kind).toBe("restarting");
    expect(pausedBarCount(pausedRow(), c.getState())).toBe(0);
    c.stop();
  });

  it("says which version it updated to on the first run after an update", () => {
    expect(updatedHeading({ autoCheck: true, version: "0.3.1", updatedFrom: "0.3.0" })).toBe(
      "Updated to rdm 0.3.1",
    );
    expect(updatedHeading({ autoCheck: true, version: "0.3.1", updatedFrom: null })).toBeNull();
    expect(updatedHeading(null)).toBeNull();
  });
});
