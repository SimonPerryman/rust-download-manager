import { useEffect, useRef, useState } from "react";
import { dragMayCarryUrl, linksFromTransfer, urlFromTransfer } from "./url";

function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || target.closest("input, textarea, select") !== null;
}

/**
 * Window-wide link intake: a URL dropped on the window, or text holding links
 * pasted while no text field is focused, is handed to `onLinks`. Returns
 * whether a URL drag is currently over the window (for the drop overlay).
 */
export function useUrlIntake(onLinks: (text: string) => void, enabled: boolean): boolean {
  const [dragging, setDragging] = useState(false);
  const depth = useRef(0);
  const onLinksRef = useRef(onLinks);
  onLinksRef.current = onLinks;

  useEffect(() => {
    const reset = () => {
      depth.current = 0;
      setDragging(false);
    };

    // dragenter/dragleave fire for every child element crossed, so count
    // nesting depth to know when the drag has really left the window.
    const onDragEnter = (e: DragEvent) => {
      if (!e.dataTransfer || !dragMayCarryUrl([...e.dataTransfer.types])) return;
      depth.current += 1;
      if (enabled) setDragging(true);
    };
    const onDragLeave = () => {
      if (depth.current === 0) return;
      depth.current -= 1;
      if (depth.current === 0) setDragging(false);
    };
    // Cancelling dragover/drop is what allows a drop at all, and also stops the
    // webview navigating to a dropped link. Text fields keep native drop.
    const onDragOver = (e: DragEvent) => {
      if (isEditable(e.target)) return;
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = enabled ? "copy" : "none";
    };
    const onDrop = (e: DragEvent) => {
      reset();
      if (isEditable(e.target)) return;
      e.preventDefault();
      const url = urlFromTransfer(e.dataTransfer);
      if (enabled && url) onLinksRef.current(url);
    };
    const onPaste = (e: ClipboardEvent) => {
      if (!enabled || isEditable(document.activeElement)) return;
      const links = linksFromTransfer(e.clipboardData);
      if (!links) return;
      e.preventDefault();
      onLinksRef.current(links);
    };

    window.addEventListener("dragenter", onDragEnter);
    window.addEventListener("dragleave", onDragLeave);
    window.addEventListener("dragover", onDragOver);
    window.addEventListener("drop", onDrop);
    window.addEventListener("dragend", reset);
    document.addEventListener("paste", onPaste);
    return () => {
      window.removeEventListener("dragenter", onDragEnter);
      window.removeEventListener("dragleave", onDragLeave);
      window.removeEventListener("dragover", onDragOver);
      window.removeEventListener("drop", onDrop);
      window.removeEventListener("dragend", reset);
      document.removeEventListener("paste", onPaste);
    };
  }, [enabled]);

  return dragging && enabled;
}
