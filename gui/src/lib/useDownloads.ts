import { useEffect, useState } from "react";
import * as api from "./api";
import type { DownloadSnapshot } from "./types";

/**
 * Every download, from a one-shot `list()` then the backend's live
 * `downloads-changed` events. `seeded` turns true once the first payload
 * (from either source) has been applied.
 */
export function useDownloads(): { downloads: DownloadSnapshot[]; seeded: boolean } {
  const [state, setState] = useState<{ downloads: DownloadSnapshot[]; seeded: boolean }>({
    downloads: [],
    seeded: false,
  });

  useEffect(() => {
    let live = true;
    // Once an event has applied, a later-resolving list() can only be older.
    let eventSeen = false;
    api
      .list()
      .then((downloads) => {
        if (live && !eventSeen) setState({ downloads, seeded: true });
      })
      .catch(() => {});
    const unlisten = api.onDownloadsChanged((downloads) => {
      if (!live) return;
      eventSeen = true;
      setState({ downloads, seeded: true });
    });
    return () => {
      live = false;
      unlisten.then((fn) => fn());
    };
  }, []);

  return state;
}
