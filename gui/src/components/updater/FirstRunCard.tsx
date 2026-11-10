import { useLayoutEffect, useRef } from "react";

type Props = {
  onAnswer: (yes: boolean) => void;
};

// The one-time "Check for updates automatically?" question, bottom left. It
// stays until answered (closing the app without answering asks again), and
// nothing is checked before then. Below the dialogs; while it shows, toasts
// and finish notifications rise above its height (styles/overlays.css), so
// they never cover it, at any window width.
export function FirstRunCard({ onAnswer }: Props) {
  const ref = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    const root = document.documentElement;
    const card = ref.current;
    if (!card) return;
    const measure = () => root.style.setProperty("--first-run-h", `${card.offsetHeight}px`);
    measure();
    root.dataset.firstRun = "";
    const observer = new ResizeObserver(measure);
    observer.observe(card);
    return () => {
      observer.disconnect();
      delete root.dataset.firstRun;
      root.style.removeProperty("--first-run-h");
    };
  }, []);

  return (
    <section
      ref={ref}
      data-testid="first-run-card"
      aria-labelledby="first-run-title"
      className="first-run-card"
    >
      <h2 id="first-run-title" className="mb-1 text-base font-semibold">
        Check for updates automatically?
      </h2>
      <p className="mb-3 text-xs text-g600">
        rdm can ask GitHub once a day whether a newer version is out. It sends nothing about you or
        your downloads. You can change this in Settings.
      </p>
      <div className="flex justify-end gap-2">
        <button className="btn btn-ghost h-7" onClick={() => onAnswer(false)}>
          No thanks
        </button>
        <button className="btn btn-primary h-7" onClick={() => onAnswer(true)}>
          Yes, check
        </button>
      </div>
    </section>
  );
}
