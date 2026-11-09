import { useLayoutEffect, useRef, useState } from "react";
import { formatSpeed } from "../../lib/format";
import { NO_LIMIT, exceeds } from "../../lib/rate";
import type { LimitEditor } from "../../lib/useLimitEditor";
import { RateInput } from "../RateInput";

// Room the popover needs below its row before it opens upwards instead.
const FLIP_PX = 120;

// The row's speed-limit popover, anchored under the actions cell. It lives
// inside the row, so its input and message belong to the row for e2e lookups.
export function RowLimitEditor({ editor }: { editor: LimitEditor }) {
  const ref = useRef<HTMLDivElement>(null);
  const [flip, setFlip] = useState(false);

  useLayoutEffect(() => {
    const row = ref.current?.closest("tr");
    const wrap = ref.current?.closest(".list-wrap");
    if (!row || !wrap) return;
    setFlip(wrap.getBoundingClientRect().bottom - row.getBoundingClientRect().bottom < FLIP_PX);
  }, []);

  const draft = editor.draft;
  if (!draft) return null;
  let message: React.ReactNode = null;
  if (editor.error) {
    message = <span className="text-danger-ink">{editor.error}</span>;
  } else if (editor.globalLimit != null && exceeds(draft, editor.globalLimit)) {
    message = `The global limit is ${formatSpeed(editor.globalLimit)} for all downloads combined, so this one won't go faster than that.`;
  } else if (draft.amount === "") {
    message = "Empty = no limit.";
  }

  return (
    <div
      ref={ref}
      className={`absolute right-0 z-3 w-max max-w-[320px] whitespace-normal rounded-lg bg-raised p-3 text-left shadow-pop ring-1 ring-line ${
        flip ? "bottom-full" : "top-full"
      }`}
    >
      <RateInput
        label="Speed limit"
        size="sm"
        autoFocus
        value={draft}
        onChange={editor.change}
        invalid={!!editor.error}
        onEnter={() => editor.commit(undefined, true)}
        onLeave={() => editor.commit()}
        onEscape={editor.cancel}
        onClear={() => {
          editor.change(NO_LIMIT);
          editor.commit(NO_LIMIT, true);
        }}
      />
      {message && <p className="mt-2 text-xs text-g600">{message}</p>}
    </div>
  );
}
