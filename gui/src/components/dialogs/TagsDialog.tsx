import { useEffect, useRef } from "react";
import type { DownloadSnapshot } from "../../lib/types";
import { TagEditor } from "../tags/TagEditor";
import { trapTab } from "./trapTab";

type Props = {
  /** The live rows being tagged; useTagsDialog closes the dialog once none is left. */
  ds: DownloadSnapshot[];
  onClose: () => void;
};

// Tags… from the menu or the selection toolbar: the tag editor for those rows.
// Each change applies at once, so the only way out is Done.
export function TagsDialog({ ds, onClose }: Props) {
  const dialog = useRef<HTMLDivElement>(null);
  const names = ds.map((d) => d.filename).join(", ");

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      if (e.key === "Tab") trapTab(e, dialog.current);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-60 grid place-items-center bg-(--backdrop) p-4"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby="tags-title"
        data-testid="tags-dialog"
        className="flex w-full max-w-[440px] flex-col rounded-lg bg-raised text-ink shadow-dialog"
      >
        <div className="px-6 pt-6">
          <h2 id="tags-title" className="text-xl font-semibold tracking-tight">
            {ds.length > 1 ? `Tags for ${ds.length} downloads` : "Tags"}
          </h2>
          <p className="mt-1 truncate text-g600" title={names}>
            {names}
          </p>
        </div>
        <div className="px-6 py-4">
          <TagEditor rows={ds} autoFocus />
        </div>
        <div className="flex items-center justify-end gap-2 rounded-b-lg bg-surface px-6 py-4">
          <button className="btn btn-primary" onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
