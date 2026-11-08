const STOPS =
  'input[type="radio"]:checked, input:not([type="radio"]):not([disabled]), textarea:not([disabled]), select:not([disabled]), button:not([disabled])';

/**
 * aria-modal promises the page behind is out of reach, so Tab cycles inside
 * the dialog (a checked radio stands for its group, as the browser's Tab does;
 * hidden fields are skipped).
 */
export function trapTab(e: KeyboardEvent, root: HTMLElement | null) {
  if (!root) return;
  const stops = Array.from(root.querySelectorAll<HTMLElement>(STOPS)).filter(
    (el) => el.offsetParent !== null,
  );
  if (stops.length === 0) return;
  const first = stops[0];
  const last = stops[stops.length - 1];
  const inside = root.contains(document.activeElement);
  if (e.shiftKey && (document.activeElement === first || !inside)) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && (document.activeElement === last || !inside)) {
    e.preventDefault();
    first.focus();
  }
}
