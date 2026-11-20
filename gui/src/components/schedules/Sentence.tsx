import type { Part } from "../../lib/schedule";

// A rule's sentence with its tag, batch and host names as chips; "+N more" carries the full
// list on hover.
export function Sentence({ parts }: { parts: readonly Part[] }) {
  return (
    <>
      {parts.map((p, i) =>
        typeof p === "string" ? (
          p
        ) : "chip" in p ? (
          <span key={i} data-testid="scope-name" className="tag-label align-middle">
            {p.chip}
          </span>
        ) : (
          <span key={i} data-testid="scope-more" title={p.title} className="tag-label align-middle">
            +{p.more} more
          </span>
        ),
      )}
    </>
  );
}
