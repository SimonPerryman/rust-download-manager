import { describe, expect, it } from "vitest";
import {
  addedMessage,
  copiedMessage,
  deletedPartialsMessage,
  emptyCopy,
  queuePlace,
  removedManyMessage,
  stoppedManyMessage,
  stopTitle,
  globalLimitLabel,
  removedMessage,
  removeBody,
  removeTitle,
  removeTitleFor,
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
    expect(removeTitle(kept)).toBe("Remove from rdm. The partial file stays on disk.");
    const failed = snap({ filename: "a.iso", status: "failed" });
    expect(removedMessage(failed)).toBe("Removed a.iso from the list.");
    expect(removeTitle(failed)).toBe("Remove from rdm. The file stays on disk.");
  });
});

describe("Remove dialog copy", () => {
  it("says files stay, kept partials stay, and failed partials go", () => {
    expect(removeTitleFor(1)).toBe("Remove this download?");
    expect(removeTitleFor(3)).toBe("Remove 3 downloads?");
    expect(removeBody([snap({ status: "completed" })])).toBe(
      "rdm forgets it for good. Downloaded files stay on disk.",
    );
    expect(
      removeBody([
        snap({ id: 1, status: "cancelled", partial_kept: true }),
        snap({ id: 2, status: "failed" }),
      ]),
    ).toBe(
      "rdm forgets them for good. Downloaded files stay on disk. Kept partial files stay too. Unfinished partial files are deleted.",
    );
  });
});

describe("viewTitle and emptyCopy for the archive and tags", () => {
  it("names Archived and tag views", () => {
    expect(viewTitle({ kind: "archived" }, [])).toBe("Archived");
    expect(viewTitle({ kind: "tag", tag: "work", archived: false }, [])).toBe("work");
    expect(viewTitle({ kind: "tag", tag: "work", archived: true }, [])).toBe("Archived › work");
    expect(emptyCopy({ kind: "archived" }, "Archived", "").title).toBe("Nothing archived");
    expect(emptyCopy({ kind: "tag", tag: "iso", archived: false }, "iso", "").title).toBe(
      "Nothing tagged iso",
    );
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

describe("bulk and queue copy", () => {
  it("counts in titles and toasts", () => {
    expect(stopTitle(1)).toBe("Stop this download?");
    expect(stopTitle(3)).toBe("Stop 3 downloads?");
    expect(stoppedManyMessage(2, true)).toBe("Stopped 2 downloads.");
    expect(stoppedManyMessage(2, false)).toBe(
      "Stopped 2 downloads and deleted their partial files.",
    );
    expect(addedMessage(3)).toBe("Added 3 downloads.");
    expect(copiedMessage(1)).toBe("Copied the link.");
    expect(copiedMessage(2)).toBe("Copied 2 links.");
    expect(deletedPartialsMessage(2)).toBe("Deleted 2 partial files.");
  });

  it("says files stay on disk only when some do", () => {
    expect(removedManyMessage([snap({ status: "failed" }), snap({ status: "failed" })])).toBe(
      "Removed 2 downloads from the list.",
    );
    expect(removedManyMessage([snap({ status: "failed" }), snap({ status: "completed" })])).toBe(
      "Removed 2 downloads from the list. Files stay on disk.",
    );
    expect(
      removedManyMessage([
        snap({ status: "cancelled", partial_kept: true }),
        snap({ status: "failed" }),
      ]),
    ).toBe("Removed 2 downloads from the list. Files stay on disk.");
  });

  it("names a waiting row's place", () => {
    expect(queuePlace(0)).toBe("Starts next");
    expect(queuePlace(1)).toBe("Waiting behind 1 other");
    expect(queuePlace(3)).toBe("Waiting behind 3 others");
  });
});
