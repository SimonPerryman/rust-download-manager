import { useCallback, useEffect, useRef, useState } from "react";

export const TOAST_MS = 3_500;

/** One info toast; `seq` changes per toast so a replacement can re-key and replay its entry. */
export interface Toast {
  seq: number;
  message: string;
}

/** The info toast slot: one at a time, a new one replaces the last, each gone after `TOAST_MS`. */
export function useToast(): {
  toast: Toast | null;
  show: (message: string) => void;
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
    (message: string) => {
      if (timer.current) clearTimeout(timer.current);
      seq.current += 1;
      setToast({ seq: seq.current, message });
      timer.current = setTimeout(dismiss, TOAST_MS);
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
