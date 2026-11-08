import { useCallback, useRef } from "react";
import * as api from "./api";
import {
  copiedMessage,
  deletedPartialMessage,
  deletedPartialsMessage,
  removedManyMessage,
  removedMessage,
} from "./copy";
import { folderOf } from "./folders";
import type { MenuCommand } from "./menu";
import { stepIndex } from "./queue";
import { isPausable, isRemovable, isStoppable, toResume } from "./rowView";
import type { DownloadSnapshot } from "./types";

type Deps = {
  downloads: readonly DownloadSnapshot[];
  /** The waiting order, first to start first. */
  queue: readonly number[];
  notify: (message: string) => void;
  fail: (e: unknown) => void;
  open: (d: DownloadSnapshot) => void;
  reveal: (d: DownloadSnapshot) => void;
  retryAll: () => void;
  /** Opens the Stop dialog for these (stoppable) ids. */
  askStop: (ids: number[]) => void;
};

/**
 * Runs a menu, toolbar or keyboard command on a set of downloads: each is the
 * per-download engine command for every row it applies to, plus its toast.
 */
export function useBulkCommands(deps: Deps): (cmd: MenuCommand, ids: readonly number[]) => void {
  // Commands run from event handlers; reading the latest deps keeps the runner stable.
  const ref = useRef(deps);
  ref.current = deps;

  return useCallback((cmd: MenuCommand, ids: readonly number[]) => {
    const { downloads, queue, notify, fail, open, reveal, retryAll, askStop } = ref.current;
    const list = ids.flatMap((id) => downloads.find((d) => d.id === id) ?? []);
    const each = (
      f: (d: DownloadSnapshot) => boolean,
      run: (d: DownloadSnapshot) => Promise<unknown>,
    ) => Promise.all(list.filter(f).map(run));
    const done = (d: DownloadSnapshot) => d.status === "completed";
    const kept = (d: DownloadSnapshot) => d.status === "cancelled" && d.partial_kept;
    const one = list[0];

    switch (cmd) {
      case "open":
        return list.filter(done).forEach(open);
      case "reveal":
        if (one && done(one)) reveal(one);
        return;
      case "pause":
        return void each(isPausable, (d) => api.pause(d.id)).catch(fail);
      case "resume":
        return void Promise.all(toResume(list).map((d) => api.resume(d.id))).catch(fail);
      case "retry":
        return void each(
          (d) => d.status === "failed",
          (d) => api.resume(d.id),
        ).catch(fail);
      case "retryAll":
        return retryAll();
      case "stop": {
        const stoppable = list.filter(isStoppable).map((d) => d.id);
        if (stoppable.length) askStop(stoppable);
        return;
      }
      case "startNext":
      case "moveUp":
      case "moveDown": {
        if (!one) return;
        const index = cmd === "startNext" ? 0 : stepIndex(queue, one.id, cmd === "moveUp" ? -1 : 1);
        if (index == null || !queue.includes(one.id)) return;
        return void api.moveQueued(one.id, index).catch(fail);
      }
      case "copyLink": {
        const urls = list.map((d) => d.url);
        return void Promise.resolve()
          .then(() => navigator.clipboard.writeText(urls.join("\n")))
          .then(() => notify(copiedMessage(urls.length)))
          .catch(fail);
      }
      case "addAgain":
        return void each(done, (d) =>
          api.addDownload(
            d.url,
            folderOf(d.target) || null,
            d.checksum,
            d.limit != null ? String(d.limit) : null,
          ),
        ).catch(fail);
      case "deletePartial": {
        const targets = list.filter(kept);
        if (!targets.length) return;
        return void each(kept, (d) => api.stop(d.id, false))
          .then(() =>
            notify(
              targets.length === 1
                ? deletedPartialMessage(targets[0])
                : deletedPartialsMessage(targets.length),
            ),
          )
          .catch(fail);
      }
      case "remove": {
        const targets = list.filter(isRemovable);
        if (!targets.length) return;
        return void each(isRemovable, (d) => api.remove(d.id))
          .then(() =>
            notify(targets.length === 1 ? removedMessage(targets[0]) : removedManyMessage(targets)),
          )
          .catch(fail);
      }
    }
  }, []);
}
