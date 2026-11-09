import { useEffect, useState } from "react";

/** An element's content size, kept current as it resizes (0 × 0 until measured). */
export function useSize(ref: React.RefObject<HTMLElement | null>): {
  width: number;
  height: number;
} {
  const [size, setSize] = useState({ width: 0, height: 0 });
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      setSize((s) =>
        s.width === Math.round(width) && s.height === Math.round(height)
          ? s
          : { width: Math.round(width), height: Math.round(height) },
      );
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref]);
  return size;
}
