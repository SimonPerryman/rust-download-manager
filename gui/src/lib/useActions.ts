import { useCallback, useEffect, useState } from "react";
import * as api from "./api";
import type { ActionsStatus, PostActions } from "./types";

/**
 * The post-download actions and where the power action stands, kept live: the backend says when
 * the status changes (every second during a countdown). `save` replaces both actions and rejects
 * with why one was refused.
 */
export function useActions(): {
  actions: PostActions | null;
  status: ActionsStatus | null;
  save: (next: PostActions) => Promise<void>;
  cancelPower: () => void;
} {
  const [actions, setActions] = useState<PostActions | null>(null);
  const [status, setStatus] = useState<ActionsStatus | null>(null);

  useEffect(() => {
    let live = true;
    api
      .getPostActions()
      .then((a) => live && setActions((prev) => prev ?? a))
      .catch(() => {});
    api
      .getActionsStatus()
      .then((s) => live && setStatus((prev) => prev ?? s))
      .catch(() => {});
    const unlisten = api.onActionsStatusChanged((s) => live && setStatus(s));
    return () => {
      live = false;
      unlisten.then((fn) => fn());
    };
  }, []);

  const save = useCallback(async (next: PostActions) => {
    await api.setPostActions(next);
    setActions(next);
    // The event follows the save; this read makes the row right even if it is late.
    api
      .getActionsStatus()
      .then(setStatus)
      .catch(() => {});
  }, []);

  const cancelPower = useCallback(() => {
    api.cancelPower().catch(() => {});
  }, []);

  return { actions, status, save, cancelPower };
}
