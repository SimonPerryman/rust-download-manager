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
  remove: (d: DownloadSnapshot) => void;
  deletePartial: (d: DownloadSnapshot) => void;
  retryAll: () => void;
}
