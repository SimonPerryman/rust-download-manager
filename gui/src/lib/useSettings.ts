import { useCallback, useEffect, useState } from "react";
import * as api from "./api";
import { mergeApplied, type SettingsField } from "./settings";
import type { Settings } from "./types";

/**
 * The engine's manager-wide settings, shared by the status bar and the
 * Settings popover. `apply` takes a `set_settings` result and merges only the
 * `fields` that call patched, so parallel responses can't roll each other back.
 */
export function useSettings(): {
  settings: Settings | null;
  apply: (applied: Settings, fields: SettingsField[]) => void;
} {
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    let live = true;
    api
      .getSettings()
      // An apply that landed first is newer than this read.
      .then((s) => live && setSettings((prev) => prev ?? s))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);

  const apply = useCallback((applied: Settings, fields: SettingsField[]) => {
    setSettings((prev) => mergeApplied(prev, applied, fields));
  }, []);

  return { settings, apply };
}
