import { DownloadMark } from "./icons";

// Drop-target hint shown while a URL is dragged over the window. Purely
// visual: pointer-events are off so the window keeps receiving the drag.
export function DropOverlay() {
  return (
    <div className="pointer-events-none fixed inset-0 z-40 p-3">
      <div className="flex h-full w-full items-center justify-center rounded-xl border-2 border-dashed border-brand/60 bg-brand/5 backdrop-blur-[1px] dark:bg-brand/10">
        <div className="flex items-center gap-2.5 rounded-md bg-card px-4 py-2.5 text-sm font-medium text-card-foreground shadow-lg">
          <DownloadMark size={15} className="text-brand" />
          Drop URL to add a download
        </div>
      </div>
    </div>
  );
}
