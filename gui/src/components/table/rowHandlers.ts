import type { DownloadSnapshot } from "../../lib/types";

/** What the table's buttons ask App to do; each is a thin api call plus its toast. */
export interface RowHandlers {
  pause: (d: DownloadSnapshot) => void;
  /** Resume, Retry and Download again: the engine's `resume` covers all three. */
  resume: (d: DownloadSnapshot) => void;
  /** Opens the Stop dialog. */
  stop: (d: DownloadSnapshot) => void;
  open: (d: DownloadSnapshot) => void;
  reveal: (d: DownloadSnapshot) => void;
  /** Locate… a missing download's file with the OS file picker and relink to it. */
  locate: (d: DownloadSnapshot) => void;
  /** Remove from list: a missing download's record only (its file is already gone). */
  forget: (d: DownloadSnapshot) => void;
  /** Remove for good (from the Archived view); may ask first. */
  remove: (d: DownloadSnapshot) => void;
  archive: (d: DownloadSnapshot) => void;
  deletePartial: (d: DownloadSnapshot) => void;
  retryAll: () => void;
}
