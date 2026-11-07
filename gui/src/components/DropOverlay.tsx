import { DownloadMark } from "./icons/DownloadMark";

// Drop-target hint shown while a URL is dragged over the window. Purely
// visual: pointer-events are off so the window keeps receiving the drag.
export function DropOverlay() {
  return (
    <div className="pointer-events-none fixed inset-0 z-40 p-3">
      <div className="flex h-full w-full items-center justify-center rounded-lg border-2 border-dashed border-accent bg-[var(--drop-tint)]">
        <div className="flex items-center gap-2 rounded-md bg-raised px-4 py-2 text-sm font-medium text-ink shadow-pop">
          <DownloadMark size={15} className="text-accent" />
          Drop URL to add a download
        </div>
      </div>
    </div>
  );
}
