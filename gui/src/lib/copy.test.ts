import { describe, expect, it } from "vitest";
import {
  emptyCopy,
  globalLimitLabel,
  removedMessage,
  removeTitle,
  retryAllMessage,
  stopDeleteHint,
  stopKeepHint,
  stoppedMessage,
  viewTitle,
} from "./copy";
import { snap } from "./testSnapshot";

const MB = 1024 * 1024;

describe("viewTitle", () => {
  const folders = [{ dir: "/home/a/isos", label: "isos", count: 2 }];

  it("names each status view", () => {
    expect(viewTitle({ kind: "status", status: "all" }, folders)).toBe("All downloads");
    expect(viewTitle({ kind: "status", status: "waiting" }, folders)).toBe("Waiting");
    expect(viewTitle({ kind: "status", status: "stopped" }, folders)).toBe("Stopped");
  });

  it("uses the folder's label, or its dir once it's gone", () => {
    expect(viewTitle({ kind: "folder", dir: "/home/a/isos" }, folders)).toBe("isos");
    expect(viewTitle({ kind: "folder", dir: "/tmp" }, folders)).toBe("/tmp");
  });
});

describe("emptyCopy", () => {
  it("invites an add for each status view", () => {
    expect(emptyCopy({ kind: "status", status: "all" }, "All downloads", "")).toEqual({
      title: "No downloads yet",
      body: "Paste a link anywhere with Ctrl+V, or use Add.",
      invitesAdd: true,
    });
    expect(emptyCopy({ kind: "status", status: "failed" }, "Failed", "").title).toBe(
      "No failed downloads",
    );
  });

  it("names the folder", () => {
    expect(emptyCopy({ kind: "folder", dir: "/a/isos" }, "isos", "").title).toBe("Nothing in isos");
  });

  it("quotes a query that matched nothing and doesn't offer Add", () => {
    const q = emptyCopy({ kind: "status", status: "done" }, "Done", "  zzq ");
    expect(q.title).toBe("Nothing matches “zzq”");
    expect(q.invitesAdd).toBe(false);
  });
});

describe("stop dialog hints", () => {
  it("names the bytes at stake", () => {
    expect(stopKeepHint(12.3 * MB)).toBe("Keeps 12.3 MB so Resume picks up where it left off.");
    expect(stopDeleteHint(12.3 * MB)).toBe("Frees 12.3 MB. Starting again downloads from zero.");
  });

  it("has a 0-byte variant", () => {
    expect(stopKeepHint(0)).toBe(
      "Nothing downloaded yet, but its place and settings stay in the list.",
    );
    expect(stopDeleteHint(0)).toBe("Starting again downloads from zero.");
  });
});

describe("stoppedMessage", () => {
  const d = snap({ filename: "a.iso", downloaded: 10 });

  it("says whether the partial was kept", () => {
    expect(stoppedMessage(d, true)).toBe("Stopped a.iso. The partial file is kept.");
    expect(stoppedMessage({ ...d, downloaded: 0 }, true)).toBe("Stopped a.iso.");
    expect(stoppedMessage(d, false)).toBe("Stopped a.iso and deleted the partial file.");
  });
});

describe("removedMessage and removeTitle", () => {
  it("say what stays on disk", () => {
    expect(removedMessage(snap({ filename: "a.iso", status: "completed" }))).toBe(
      "Removed a.iso from the list. The file stays on disk.",
    );
    const kept = snap({ filename: "a.iso", status: "cancelled", partial_kept: true });
    expect(removedMessage(kept)).toBe(
      "Removed a.iso from the list. Its partial file stays on disk.",
    );
    expect(removeTitle(kept)).toBe("Remove from list. The partial file stays on disk.");
    const failed = snap({ filename: "a.iso", status: "failed" });
    expect(removedMessage(failed)).toBe("Removed a.iso from the list.");
    expect(removeTitle(failed)).toBe("Remove from list");
  });
});

describe("retryAllMessage", () => {
  it("names a lone download, counts several", () => {
    expect(retryAllMessage([snap({ filename: "a.iso" })])).toBe("Retrying a.iso");
    expect(retryAllMessage([snap({ id: 1 }), snap({ id: 2 })])).toBe("Retrying 2 failed downloads");
  });
});

describe("globalLimitLabel", () => {
  it("reads as a phrase", () => {
    expect(globalLimitLabel(null)).toBe("No speed limit");
    expect(globalLimitLabel(2 * MB)).toBe("Limit 2.00 MB/s");
  });
});
