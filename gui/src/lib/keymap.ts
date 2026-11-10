// The table's keyboard map, as data: which key does what while the table has
// focus. useTableInput runs the command; this only names it.

export type KeyCommand =
  | { kind: "move"; to: "up" | "down" | "first" | "last"; extend: boolean }
  | { kind: "selectAll" }
  /** Space: pause what runs in the selection, else resume what's paused. */
  | { kind: "pauseOrResume" }
  /** Enter: open the cursor row's finished file. */
  | { kind: "open" }
  | { kind: "remove" }
  | { kind: "menu" }
  /** Alt+↑ / Alt+↓: move the cursor's waiting row one place in the queue. */
  | { kind: "reorder"; delta: -1 | 1 }
  | { kind: "clear" };

export type KeyLike = Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "shiftKey" | "altKey">;

export function keyCommand(e: KeyLike): KeyCommand | null {
  const ctrl = e.ctrlKey || e.metaKey;
  if (e.altKey) {
    if (ctrl || e.shiftKey) return null;
    if (e.key === "ArrowUp") return { kind: "reorder", delta: -1 };
    if (e.key === "ArrowDown") return { kind: "reorder", delta: 1 };
    return null;
  }
  switch (e.key) {
    case "ArrowUp":
      return ctrl ? null : { kind: "move", to: "up", extend: e.shiftKey };
    case "ArrowDown":
      return ctrl ? null : { kind: "move", to: "down", extend: e.shiftKey };
    case "Home":
      return { kind: "move", to: "first", extend: e.shiftKey };
    case "End":
      return { kind: "move", to: "last", extend: e.shiftKey };
    case "a":
    case "A":
      return ctrl && !e.shiftKey ? { kind: "selectAll" } : null;
    case " ":
      return ctrl ? null : { kind: "pauseOrResume" };
    case "Enter":
      return ctrl || e.shiftKey ? null : { kind: "open" };
    case "Delete":
      return ctrl || e.shiftKey ? null : { kind: "remove" };
    case "F10":
      return e.shiftKey && !ctrl ? { kind: "menu" } : null;
    case "ContextMenu":
      return { kind: "menu" };
    case "Escape":
      return { kind: "clear" };
    default:
      return null;
  }
}
