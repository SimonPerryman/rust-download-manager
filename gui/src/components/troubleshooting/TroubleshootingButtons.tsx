import { useEffect, useState } from "react";
import * as api from "../../lib/api";
import { useCopied } from "../../lib/useCopied";
import { Check } from "../icons/Check";
import { Copy } from "../icons/Copy";
import { Folder } from "../icons/Folder";

type Size = "sm" | "md";

const btnClass = (size: Size) => `btn btn-ghost${size === "sm" ? " btn-sm" : ""}`;
const iconSize = (size: Size) => (size === "sm" ? 14 : 16);

/** Opens the logs folder; its tooltip is the folder's path. */
export function OpenLogsFolderButton({
  size,
  onError,
}: {
  size: Size;
  /** A problem to show, or `null` to clear the last one as an action starts. */
  onError: (message: string | null) => void;
}) {
  const [folder, setFolder] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    api.logsFolder().then(
      (dir) => live && setFolder(dir),
      () => {},
    );
    return () => {
      live = false;
    };
  }, []);
  return (
    <button
      type="button"
      className={btnClass(size)}
      data-testid="open-logs-folder"
      title={folder ?? undefined}
      onClick={() => {
        onError(null);
        api.openLogsFolder().catch((e) => onError(`Couldn't open the logs folder: ${e}`));
      }}
    >
      <Folder size={iconSize(size)} />
      Open logs folder
    </button>
  );
}

/**
 * Puts the diagnostics report on the clipboard, with `extra` (a caught
 * error) in it, and says "Copied" for a moment.
 */
export function CopyDiagnosticsButton({
  size,
  extra,
  onError,
}: {
  size: Size;
  extra?: string | null;
  /** A problem to show, or `null` to clear the last one as an action starts. */
  onError: (message: string | null) => void;
}) {
  const [copied, markCopied] = useCopied<true>();
  const copy = () => {
    onError(null);
    void api
      .diagnostics(extra)
      .then((text) => navigator.clipboard.writeText(text))
      .then(() => markCopied(true))
      .catch((e) => onError(`Couldn't copy diagnostics: ${e}`));
  };
  return (
    <>
      <button
        type="button"
        className={`${btnClass(size)}${copied ? " copied" : ""}`}
        data-testid="copy-diagnostics"
        onClick={copy}
      >
        {copied ? (
          <Check size={iconSize(size)} strokeWidth={2.5} />
        ) : (
          <Copy size={iconSize(size)} />
        )}
        {copied ? "Copied" : "Copy diagnostics"}
      </button>
      <span className="sr-only" role="status">
        {copied ? "Diagnostics copied" : ""}
      </span>
    </>
  );
}
