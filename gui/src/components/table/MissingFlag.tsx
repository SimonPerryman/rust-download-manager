import { FileQuestion } from "../icons/FileQuestion";

// A finished row's file is gone: an icon and the words, so colour is never the only signal.
export function MissingFlag() {
  return (
    <span className="missing-flag" data-testid="missing-flag">
      <FileQuestion size={14} strokeWidth={2} />
      File missing
    </span>
  );
}
