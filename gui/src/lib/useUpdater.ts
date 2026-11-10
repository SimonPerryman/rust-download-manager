import { useEffect, useState, useSyncExternalStore } from "react";
import { UpdateController, type UpdaterState } from "./updater";

/** The updater's state and its controller, started on mount and stopped on unmount. */
export function useUpdater(): [UpdaterState, UpdateController] {
  const [controller] = useState(() => new UpdateController());
  const state = useSyncExternalStore(controller.subscribe, controller.getState);
  useEffect(() => {
    void controller.start().catch(() => {});
    return () => controller.stop();
  }, [controller]);
  return [state, controller];
}
