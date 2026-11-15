import { matchBadge } from "../../lib/copy";
import type { MatchReason } from "../../lib/types";
import { Check } from "../icons/Check";
import { Info } from "../icons/Info";

// Why a found file may be the one. Words and an icon, so colour is never the only signal.
export function MatchBadge({ reason }: { reason: MatchReason }) {
  const sure = reason === "SameNameAndSize";
  return (
    <span className={`find-badge ${sure ? "find-badge-ok" : "find-badge-maybe"}`}>
      {sure ? <Check size={12} strokeWidth={2.25} /> : <Info size={12} />}
      {matchBadge[reason]}
    </span>
  );
}
