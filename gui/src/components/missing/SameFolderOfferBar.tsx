import { sameFolderOfferLine } from "../../lib/copy";
import type { SameFolderOffer } from "../../lib/types";
import { Folder } from "../icons/Folder";
import { X } from "../icons/X";

type Props = {
  /** The engine's offer after a Locate…; null hides the bar. */
  offer: SameFolderOffer | null;
  onAccept: () => void;
  onDismiss: () => void;
};

// After Locate…: other missing downloads from the same old folder were found in the new one.
// Nothing is relinked until the user accepts; ignoring or dismissing leaves them as they are.
export function SameFolderOfferBar({ offer, onAccept, onDismiss }: Props) {
  if (!offer) return null;
  return (
    <div
      role="status"
      data-testid="same-folder-offer"
      className="flex min-w-0 items-center gap-3 border-b border-line bg-primary-soft px-4 py-2 text-sm text-ink"
    >
      <Folder size={16} className="shrink-0" />
      <span className="min-w-0">
        <strong className="font-semibold">{sameFolderOfferLine(offer.count)}</strong>
        <span
          className="block truncate font-mono text-xs text-g600"
          title={`${offer.old_folder} → ${offer.new_folder}`}
        >
          {offer.old_folder} → {offer.new_folder}
        </span>
      </span>
      <span className="ml-auto flex shrink-0 items-center gap-2">
        <button className="btn btn-line h-7" onClick={onAccept}>
          Relink {offer.count === 1 ? "it" : `all ${offer.count}`}
        </button>
        <button
          className="icon-btn h-7 w-7"
          aria-label="Dismiss"
          title="Dismiss"
          onClick={onDismiss}
        >
          <X size={14} />
        </button>
      </span>
    </div>
  );
}
