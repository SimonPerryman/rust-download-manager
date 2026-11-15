import { sameFolderOfferWords } from "../../lib/copy";
import type { SameFolderOffer } from "../../lib/types";
import { Folder } from "../icons/Folder";
import { X } from "../icons/X";

type Props = {
  /** The engine's offer after a Locate…; null hides the bar. */
  offer: SameFolderOffer | null;
  onSearch: () => void;
  onDismiss: () => void;
};

// After Locate…: other missing files were in the same old folder. Search looks for them; nothing is
// relinked until the user reviews the results. Dismiss leaves them as they are.
export function SameFolderOfferBar({ offer, onSearch, onDismiss }: Props) {
  if (!offer) return null;
  const { noun, verb, pronoun } = sameFolderOfferWords(offer.count);
  return (
    <div
      role="status"
      data-testid="same-folder-offer"
      className="flex min-w-0 items-center gap-3 border-b border-line bg-primary-soft px-4 py-2 text-sm text-ink"
    >
      <Folder size={16} className="shrink-0" />
      <span className="min-w-0 flex-1 truncate">
        <strong className="font-semibold">{noun}</strong> {verb} in{" "}
        <span className="font-mono text-xs" title={offer.old_folder}>
          {offer.old_folder}
        </span>
        . Search{" "}
        <span className="font-mono text-xs" title={offer.new_folder}>
          {offer.new_folder}
        </span>{" "}
        for {pronoun}?
      </span>
      <span className="flex shrink-0 items-center gap-2">
        <button className="btn btn-primary btn-sm" onClick={onSearch}>
          Search
        </button>
        <button
          className="icon-btn icon-btn-sm"
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
