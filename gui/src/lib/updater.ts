// The in-app updater's frontend: its words, and a controller for when to
// check and what the notice shows. The checking, downloading, verifying and
// installing all happen in the GUI crate; this only sequences its commands.

import * as api from "./api";
import { formatBytes } from "./format";
import type {
  DownloadSnapshot,
  InstallWhen,
  UpdateInfo,
  UpdatePrefs,
  UpdateProgress,
} from "./types";

/** How long after an automatic check the next one is due. */
export const CHECK_EVERY_MS = 24 * 60 * 60 * 1000;
/**
 * How often the controller looks at the clock to see whether a check is due:
 * comparing wall-clock time, so a machine asleep past the due time checks soon
 * after it wakes instead of a full day later.
 */
export const CHECK_TICK_MS = 60 * 60 * 1000;

// ---------- words ----------

const downloads = (n: number) => (n === 1 ? "1 download" : `${n} downloads`);

/** Downloads Restart now would pause: those running or waiting to run. */
export function willPauseCount(ds: readonly DownloadSnapshot[]): number {
  return ds.filter((d) => d.status === "downloading" || d.status === "queued").length;
}

/** The notice's warning line under the version; null when nothing would pause. */
export function willPauseLine(n: number): string | null {
  if (n === 0) return null;
  const them = n === 1 ? "it" : "them";
  return `${downloads(n)} will pause; you can resume ${them} after the restart.`;
}

/** Downloads an update restart paused that are still paused. */
export function pausedForUpdateCount(ds: readonly DownloadSnapshot[]): number {
  return ds.filter((d) => d.paused_for_update).length;
}

/**
 * How many downloads the after-update bar offers to resume: none once it's
 * been answered, or while this process is itself restarting into an update
 * (the pause it shows then is the one the next start offers).
 */
export function pausedBarCount(ds: readonly DownloadSnapshot[], state: UpdaterState): number {
  const n = state.notice;
  const restarting = n.kind === "restarting" || (n.kind === "installing" && n.when === "now");
  return restarting || state.pausedAnswered ? 0 : pausedForUpdateCount(ds);
}

/** The after-update bar's line, e.g. "2 downloads were paused for the update." */
export function pausedForUpdateLine(n: number): string {
  return `${downloads(n)} ${n === 1 ? "was" : "were"} paused for the update.`;
}

/** The after-update bar's heading, on the first run after an update; else null. */
export function updatedHeading(prefs: UpdatePrefs | null): string | null {
  return prefs?.updatedFrom ? `Updated to rdm ${prefs.version}` : null;
}

/** "4.10 MB of 8.90 MB" (or just the bytes so far when the size is unknown). */
export function progressText(p: UpdateProgress | null): string {
  if (!p) return "";
  const done = formatBytes(p.downloaded);
  return p.total ? `${done} of ${formatBytes(p.total)}` : done;
}

/** The fraction of the update downloaded, 0–1; null while the size is unknown. */
export function progressFraction(p: UpdateProgress | null): number | null {
  return p?.total ? Math.min(1, p.downloaded / p.total) : null;
}

/** A sentence as it reads after a middot: lower case first, unless it starts with an acronym. */
export function afterMiddot(text: string): string {
  return /^[A-Z][^A-Z]/.test(text) ? text[0].toLowerCase() + text.slice(1) : text;
}

// ---------- state ----------

/** What the update notice under the toolbar shows. */
export type Notice =
  | { kind: "hidden" }
  | { kind: "available"; info: UpdateInfo }
  | { kind: "installing"; version: string; when: InstallWhen; progress: UpdateProgress | null }
  | { kind: "staged"; version: string }
  | { kind: "restarting"; version: string }
  | { kind: "failed"; version: string; when: InstallWhen; message: string };

/** A button or link on the notice. */
export type NoticeAction =
  | "later"
  | "install-on-quit"
  | "restart"
  | "download"
  | "restart-instead"
  | "dismiss"
  | "retry";

export const NOTICE_ACTION_LABELS: Record<NoticeAction, string> = {
  later: "Later",
  "install-on-quit": "Install when I quit",
  restart: "Restart now",
  download: "Download",
  "restart-instead": "Restart now instead",
  dismiss: "Dismiss",
  retry: "Try again",
};

/** What the notice shows for one state. */
export interface NoticeView {
  tone: "info" | "done" | "error";
  title: string;
  /** A "What's new" link after the title. */
  notes: boolean;
  /** Quieter text after the title (and link), after a middot. */
  detail: string | null;
  /** Lines of their own under the title: what Restart now pauses, an update already staged. */
  lines: string[];
  /** Downloading the update: a progress bar, with `progress` once it's known. */
  downloading: boolean;
  progress: UpdateProgress | null;
  /** In order, left to right; the last is the primary action where there is one. */
  actions: NoticeAction[];
}

/** What else the notice needs to know. */
export interface NoticeContext {
  /** Downloads running or waiting, which Restart now would pause. */
  willPause: number;
  /** The version staged to install at quit, if any. */
  staged: string | null;
}

/** The notice for `n`; null when hidden. */
export function noticeView(n: Notice, ctx: NoticeContext): NoticeView | null {
  const base = {
    notes: false,
    detail: null,
    lines: [] as string[],
    downloading: false,
    progress: null,
    actions: [],
  };
  const pauses = (lines: (string | null)[]) => lines.filter((l): l is string => l !== null);
  switch (n.kind) {
    case "hidden":
      return null;
    case "available": {
      const staged =
        ctx.staged && ctx.staged !== n.info.version
          ? `rdm ${ctx.staged} will install when you quit, unless you install rdm ${n.info.version}.`
          : null;
      return n.info.canInstall
        ? {
            ...base,
            tone: "info",
            title: `rdm ${n.info.version} is available`,
            notes: true,
            lines: pauses([willPauseLine(ctx.willPause), staged]),
            actions: ["later", "install-on-quit", "restart"],
          }
        : {
            ...base,
            tone: "info",
            title: `rdm ${n.info.version} is available`,
            notes: true,
            detail: "get the new .deb from the release page.",
            actions: ["later", "download"],
          };
    }
    case "installing":
      return {
        ...base,
        tone: "info",
        title: `Downloading rdm ${n.version}…`,
        downloading: true,
        progress: n.progress,
      };
    case "staged":
      return {
        ...base,
        tone: "done",
        title: `rdm ${n.version} will install when you quit`,
        detail: "downloads keep going until then.",
        lines: pauses([willPauseLine(ctx.willPause)]),
        actions: ["restart-instead"],
      };
    case "restarting":
      return { ...base, tone: "info", title: `Restarting into rdm ${n.version}…` };
    case "failed":
      return {
        ...base,
        tone: "error",
        title: `Couldn't install rdm ${n.version}`,
        detail: afterMiddot(n.message),
        lines: n.when === "now" ? pauses([willPauseLine(ctx.willPause)]) : [],
        actions: ["dismiss", "retry"],
      };
  }
}

/** The result line under Settings' Check for updates. */
export type CheckResult =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current" }
  | { kind: "available"; version: string }
  | { kind: "staged"; version: string }
  | { kind: "error"; message: string };

/** Settings' result line for `r`; null for none. */
export function checkResultLine(r: CheckResult): string | null {
  switch (r.kind) {
    case "idle":
      return null;
    case "checking":
      return "Checking…";
    case "current":
      return "You're up to date.";
    case "available":
      return `rdm ${r.version} is available (see the bar above the list).`;
    case "staged":
      return `rdm ${r.version} will install when you quit.`;
    case "error":
      return `Couldn't check for updates: ${r.message}`;
  }
}

export interface UpdaterState {
  /** Null until loaded from the backend. */
  prefs: UpdatePrefs | null;
  notice: Notice;
  check: CheckResult;
  /** The version staged to install at quit, if any. */
  staged: string | null;
  /** Resume all or dismiss was chosen on the after-update bar. */
  pausedAnswered: boolean;
  /** Checks settled this session, automatic or not (a hook for the e2e). */
  checksDone: number;
}

/** The backend commands the controller uses (the api module; a fake in tests). */
export type UpdaterApi = Pick<
  typeof api,
  | "updatePrefs"
  | "setAutoCheck"
  | "checkForUpdate"
  | "installUpdate"
  | "stagedUpdate"
  | "onUpdateProgress"
  | "resumePausedForUpdate"
  | "forgetPausedForUpdate"
>;

/**
 * When to check and what the notice shows. Nothing is checked until the
 * first-run question is answered; with automatic checks on, one check runs on
 * start and then once `CHECK_EVERY_MS` has passed. A failed automatic check
 * shows nothing (the backend logs it); Check for updates always runs and
 * reports.
 */
export class UpdateController {
  private state: UpdaterState = {
    prefs: null,
    notice: { kind: "hidden" },
    check: { kind: "idle" },
    staged: null,
    pausedAnswered: false,
    checksDone: 0,
  };
  private listeners = new Set<() => void>();
  private timer: ReturnType<typeof setInterval> | null = null;
  private unlisten: (() => void) | null = null;
  /** Bumped by `stop`, so a call answered after it changes nothing. */
  private generation = 0;
  /** When the last automatic check started (wall clock, ms). */
  private lastAutoCheck = 0;

  constructor(private readonly backend: UpdaterApi = api) {}

  getState = (): UpdaterState => this.state;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  private set(patch: Partial<UpdaterState>) {
    this.state = { ...this.state, ...patch };
    for (const l of this.listeners) l();
  }

  private setNotice(notice: Notice) {
    this.set({ notice });
  }

  private settled() {
    this.set({ checksDone: this.state.checksDone + 1 });
  }

  /** Load the preferences and any staged update; start checking if automatic checks are on. */
  async start(): Promise<void> {
    const gen = this.generation;
    const unlisten = await this.backend
      .onUpdateProgress((progress) => {
        const n = this.state.notice;
        if (n.kind === "installing") this.setNotice({ ...n, progress });
      })
      .catch(() => null);
    if (gen !== this.generation) {
      unlisten?.();
      return;
    }
    this.unlisten = unlisten;
    const [prefs, staged] = await Promise.all([
      this.backend.updatePrefs(),
      this.backend.stagedUpdate().catch(() => null),
    ]);
    if (gen !== this.generation) return;
    this.set({
      prefs,
      staged,
      notice: staged ? { kind: "staged", version: staged } : this.state.notice,
    });
    if (prefs.autoCheck === true) {
      this.schedule();
      await this.autoCheck();
    }
  }

  /** Stop the daily check and ignore any call still in flight. */
  stop(): void {
    this.generation += 1;
    this.unschedule();
    this.unlisten?.();
    this.unlisten = null;
  }

  private schedule() {
    if (this.timer) return;
    this.timer = setInterval(() => {
      if (Date.now() - this.lastAutoCheck >= CHECK_EVERY_MS) void this.autoCheck();
    }, CHECK_TICK_MS);
  }

  private unschedule() {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  /** The first-run answer: save it, and on Yes check right away. Rejects if it couldn't be saved. */
  answer(yes: boolean): Promise<void> {
    return this.setAutoCheck(yes, yes);
  }

  /**
   * Settings' "Check for updates automatically": turning it on starts the
   * daily check, turning it off drops a check still running. Rejects (with
   * the setting as it was) if it couldn't be saved.
   */
  async setAutoCheck(on: boolean, checkNow = false): Promise<void> {
    const prefs = this.state.prefs;
    if (!prefs) return;
    const gen = this.generation;
    this.set({ prefs: { ...prefs, autoCheck: on } });
    try {
      await this.backend.setAutoCheck(on);
    } catch (e) {
      if (gen === this.generation) this.set({ prefs });
      throw e;
    }
    if (gen !== this.generation) return;
    if (!on) {
      this.unschedule();
      return;
    }
    this.schedule();
    if (checkNow) await this.autoCheck();
  }

  private async autoCheck(): Promise<void> {
    const gen = this.generation;
    this.lastAutoCheck = Date.now();
    let info: UpdateInfo | null = null;
    let ok = true;
    try {
      info = await this.backend.checkForUpdate();
    } catch {
      ok = false;
    }
    if (gen !== this.generation) return;
    // Turned off while it ran: its answer is no longer wanted.
    if (ok && this.state.prefs?.autoCheck === true) this.found(info);
    this.settled();
  }

  /** Settings' Check for updates: runs whatever the setting, and says how it went. */
  async checkNow(): Promise<void> {
    const gen = this.generation;
    this.set({ check: { kind: "checking" } });
    let info: UpdateInfo | null;
    try {
      info = await this.backend.checkForUpdate();
    } catch (e) {
      if (gen === this.generation) {
        this.set({ check: { kind: "error", message: String(e) } });
        this.settled();
      }
      return;
    }
    if (gen !== this.generation) return;
    this.found(info);
    const check: CheckResult = !info
      ? { kind: "current" }
      : info.version === this.state.staged
        ? { kind: "staged", version: info.version }
        : { kind: "available", version: info.version };
    this.set({ check });
    this.settled();
  }

  /** Forget Settings' last result line (unless a check is running). */
  clearCheckResult(): void {
    if (this.state.check.kind !== "checking") this.set({ check: { kind: "idle" } });
  }

  /**
   * A check's answer: show it, unless an install is under way, it's the
   * staged one, or an install of that same version failed and still says so.
   */
  private found(info: UpdateInfo | null) {
    const n = this.state.notice;
    if (n.kind === "installing" || n.kind === "restarting") return;
    if (n.kind === "failed" && (!info || info.version === n.version)) return;
    if (info && info.version === this.state.staged) {
      this.setNotice({ kind: "staged", version: info.version });
    } else if (info) {
      this.setNotice({ kind: "available", info });
    } else if (n.kind === "available") {
      this.setNotice({ kind: "hidden" });
    }
  }

  /** Restart now / Install when I quit (and Restart now instead, and Try again). */
  async install(when: InstallWhen): Promise<void> {
    const n = this.state.notice;
    const version =
      n.kind === "available"
        ? n.info.version
        : n.kind === "failed" || n.kind === "staged"
          ? n.version
          : null;
    if (!version) return;
    const gen = this.generation;
    this.setNotice({ kind: "installing", version, when, progress: null });
    try {
      await this.backend.installUpdate(when, version);
    } catch (e) {
      if (gen === this.generation)
        this.setNotice({ kind: "failed", version, when, message: String(e) });
      return;
    }
    if (gen !== this.generation) return;
    if (when === "on-quit") {
      this.set({ staged: version, notice: { kind: "staged", version } });
    } else {
      this.set({ staged: null, notice: { kind: "restarting", version } });
    }
  }

  /** Try again after a failed install: the same action. */
  retry(): Promise<void> {
    const n = this.state.notice;
    return n.kind === "failed" ? this.install(n.when) : Promise.resolve();
  }

  /** Later: hide the notice until the next check. */
  later(): void {
    if (this.state.notice.kind === "available") this.setNotice({ kind: "hidden" });
  }

  /** Dismiss a failed install's message (back to the staged update, if one is waiting). */
  dismiss(): void {
    if (this.state.notice.kind !== "failed") return;
    const staged = this.state.staged;
    this.setNotice(staged ? { kind: "staged", version: staged } : { kind: "hidden" });
  }

  /** The after-update bar's Resume all: hides it at once; rejects (and shows it again) on failure. */
  resumePaused(): Promise<void> {
    return this.answerPaused(() => this.backend.resumePausedForUpdate());
  }

  /** The after-update bar's ✕: leaves those downloads paused and hides it. */
  forgetPaused(): Promise<void> {
    return this.answerPaused(() => this.backend.forgetPausedForUpdate());
  }

  private async answerPaused(call: () => Promise<void>): Promise<void> {
    this.set({ pausedAnswered: true });
    try {
      await call();
    } catch (e) {
      this.set({ pausedAnswered: false });
      throw e;
    }
  }
}
