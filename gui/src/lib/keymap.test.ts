import { describe, expect, it } from "vitest";
import { keyCommand, type KeyLike } from "./keymap";

const key = (k: string, mods: Partial<KeyLike> = {}): KeyLike => ({
  key: k,
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  ...mods,
});

describe("keyCommand", () => {
  it("moves with the arrows, Home and End, extending with Shift", () => {
    expect(keyCommand(key("ArrowDown"))).toEqual({ kind: "move", to: "down", extend: false });
    expect(keyCommand(key("ArrowUp", { shiftKey: true }))).toEqual({
      kind: "move",
      to: "up",
      extend: true,
    });
    expect(keyCommand(key("Home"))).toEqual({ kind: "move", to: "first", extend: false });
    expect(keyCommand(key("End", { shiftKey: true }))).toEqual({
      kind: "move",
      to: "last",
      extend: true,
    });
  });

  it("reorders with Alt+↑ and Alt+↓ only", () => {
    expect(keyCommand(key("ArrowUp", { altKey: true }))).toEqual({ kind: "reorder", delta: -1 });
    expect(keyCommand(key("ArrowDown", { altKey: true }))).toEqual({ kind: "reorder", delta: 1 });
    expect(keyCommand(key("ArrowDown", { altKey: true, shiftKey: true }))).toBeNull();
    expect(keyCommand(key("Delete", { altKey: true }))).toBeNull();
  });

  it("maps the action keys", () => {
    expect(keyCommand(key(" "))).toEqual({ kind: "pauseOrResume" });
    expect(keyCommand(key("Enter"))).toEqual({ kind: "open" });
    expect(keyCommand(key("Delete"))).toEqual({ kind: "remove" });
    expect(keyCommand(key("Escape"))).toEqual({ kind: "clear" });
    expect(keyCommand(key("a", { ctrlKey: true }))).toEqual({ kind: "selectAll" });
    expect(keyCommand(key("a", { metaKey: true }))).toEqual({ kind: "selectAll" });
  });

  it("opens the menu with Shift+F10 or the menu key, not F10 alone", () => {
    expect(keyCommand(key("F10", { shiftKey: true }))).toEqual({ kind: "menu" });
    expect(keyCommand(key("ContextMenu"))).toEqual({ kind: "menu" });
    expect(keyCommand(key("F10"))).toBeNull();
  });

  it("leaves other keys and chords alone", () => {
    expect(keyCommand(key("a"))).toBeNull();
    expect(keyCommand(key("ArrowDown", { ctrlKey: true }))).toBeNull();
    expect(keyCommand(key("Enter", { ctrlKey: true }))).toBeNull();
    expect(keyCommand(key("x"))).toBeNull();
  });
});
