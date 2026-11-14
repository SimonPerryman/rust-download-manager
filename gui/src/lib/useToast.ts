import { useCallback, useEffect, useRef, useState } from "react";

export const TOAST_MS = 3_500;

/** One info toast; `seq` changes per toast so a replacement can re-key and replay its entry. */
export interface Toast {
  seq: number;
  message: string;
  /** A button on the toast; a toast with one stays until dismissed, so there is time to click. */
  action?: ToastAction;
}

export interface ToastAction {
  label: string;
  run: () => void;
}

/** The info toast slot: one at a time, a new one replaces the last, each gone after `TOAST_MS` unless it has an action. */
export function useToast(): {
  toast: Toast | null;
  show: (message: string, action?: ToastAction) => void;
  dismiss: () => void;
} {
  const [toast, setToast] = useState<Toast | null>(null);
  const seq = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const dismiss = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    setToast(null);
  }, []);

  const show = useCallback(
    (message: string, action?: ToastAction) => {
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
      seq.current += 1;
      setToast({ seq: seq.current, message, action });
      if (!action) timer.current = setTimeout(dismiss, TOAST_MS);
    },
    [dismiss],
  );

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  return { toast, show, dismiss };
}
