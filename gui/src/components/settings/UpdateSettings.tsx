import { useEffect, useRef } from "react";
import { checkResultLine, type CheckResult } from "../../lib/updater";
import type { UpdatePrefs } from "../../lib/types";
import { Check } from "../icons/Check";

export type UpdateSettingsProps = {
  /** Null until loaded. */
  prefs: UpdatePrefs | null;
  check: CheckResult;
  onCheck: () => void;
  onAutoCheck: (on: boolean) => void;
  /** The popover closed: its result line can go. */
  onLeave: () => void;
};

const TONE: Record<CheckResult["kind"], string> = {
  idle: "",
  checking: "text-g600",
  current: "text-success-ink",
  staged: "text-success-ink",
  available: "text-primary-ink",
  error: "text-danger-ink",
};

// The settings popover's Updates section: this version, Check for updates
// (which always runs) with its result, and the automatic-check setting.
export function UpdateSettings({
  prefs,
  check,
  onCheck,
  onAutoCheck,
  onLeave,
}: UpdateSettingsProps) {
  const leave = useRef(onLeave);
  leave.current = onLeave;
  useEffect(() => () => leave.current(), []);

  const line = checkResultLine(check);
  const ok = check.kind === "current" || check.kind === "staged";
  return (
    <div className="space-y-2" data-testid="update-settings">
      <div className="flex items-center justify-between gap-3">
        <p className="text-sm text-ink" data-testid="app-version">
          {prefs ? `rdm ${prefs.version}` : "rdm"}
        </p>
        <button
          className="btn btn-line btn-sm"
          disabled={check.kind === "checking"}
          onClick={onCheck}
        >
          Check for updates
        </button>
      </div>
      {/* Always mounted, so a screen reader hears each new result. */}
      <p
        role="status"
        data-testid="update-check-result"
        data-result={check.kind}
        className={`flex items-start gap-1 text-xs ${TONE[check.kind]}`}
      >
        {ok && <Check size={14} className="mt-px shrink-0" />}
        <span>{line ?? ""}</span>
      </p>
      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          aria-label="Check for updates automatically"
          checked={prefs?.autoCheck === true}
          disabled={!prefs}
          onChange={(e) => onAutoCheck(e.target.checked)}
        />
        Check for updates automatically
      </label>
    </div>
  );
}
