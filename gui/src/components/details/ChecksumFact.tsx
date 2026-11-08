import { useEffect, useRef, useState } from "react";
import * as api from "../../lib/api";
import { checksumView, isSha256, NOT_A_HASH, verifyErrorText } from "../../lib/details";
import type { DownloadSnapshot } from "../../lib/types";
import { ShieldCheck } from "../icons/ShieldCheck";
import { ShieldTick } from "../icons/ShieldTick";

const TONE = {
  ok: "font-medium text-success-ink",
  checking: "font-medium text-primary-ink",
  bad: "font-medium text-danger-ink",
  plain: "",
};

// The checksum fact, and on a finished file the Verify checksum form. The
// result arrives as the snapshot's checksum_state; only a refusal to start
// shows here.
export function ChecksumFact({ d }: { d: DownloadSnapshot }) {
  const view = checksumView(d);
  const [open, setOpen] = useState(false);
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const factRef = useRef<HTMLDivElement>(null);
  const openerRef = useRef<HTMLButtonElement>(null);
  // Closing the form unmounts the focused input, and a check in progress hides
  // the opener too, so focus would fall to <body>. Keep it on this fact until
  // the opener is back, unless the user has moved focus elsewhere.
  const refocus = useRef(false);
  useEffect(() => {
    if (open || !refocus.current) return;
    const active = document.activeElement;
    const ours =
      !active ||
      active === document.body ||
      active === factRef.current ||
      active === openerRef.current;
    if (!ours) {
      refocus.current = false;
      return;
    }
    (openerRef.current ?? factRef.current)?.focus();
  }, [open, view.canVerify]);

  const close = () => {
    refocus.current = true;
    setOpen(false);
    setError(null);
  };

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!isSha256(value)) {
      setError(NOT_A_HASH);
      return;
    }
    setBusy(true);
    api
      .verifyChecksum(d.id, value.trim())
      .then(() => {
        setValue("");
        close();
      })
      .catch((err) => setError(verifyErrorText(err)))
      .finally(() => setBusy(false));
  };

  const Icon = d.checksum_state === "verified" ? ShieldTick : ShieldCheck;
  return (
    <div
      ref={factRef}
      tabIndex={-1}
      className="fact"
      data-testid="checksum"
      data-state={d.checksum_state}
    >
      <Icon size={16} className="fact-icon" />
      <div className="min-w-0">
        <div className={TONE[view.tone]} role="status" aria-live="polite">
          {view.text}
        </div>
        {view.hash && <span className="hash font-mono">{view.hash}</span>}
        {view.hint && <div className="text-xs text-g400">{view.hint}</div>}
        {view.canVerify &&
          (open ? (
            <>
              <form className="verify" onSubmit={submit}>
                <input
                  className="font-mono"
                  aria-label="SHA-256 hash to compare"
                  placeholder="Paste a SHA-256 hash"
                  autoComplete="off"
                  spellCheck={false}
                  autoFocus
                  value={value}
                  aria-invalid={error ? true : undefined}
                  onChange={(e) => {
                    setValue(e.target.value);
                    setError(null);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Escape") close();
                  }}
                />
                <button
                  className="btn btn-soft btn-sm"
                  type="submit"
                  disabled={busy || !value.trim()}
                >
                  Verify
                </button>
              </form>
              {error && (
                <div className="verify-err" role="alert" data-testid="verify-error">
                  {error}
                </div>
              )}
            </>
          ) : (
            <button
              ref={openerRef}
              className="btn-link mt-1"
              onClick={() => {
                refocus.current = false;
                setOpen(true);
              }}
            >
              Verify checksum
            </button>
          ))}
      </div>
    </div>
  );
}
