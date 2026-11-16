// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { entriesFrom, type Known } from "../../lib/batch";
import type { ParsedList } from "../../lib/types";
import { LinkPreview } from "./LinkPreview";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const url = (n: number) => `https://a.example/lesson${String(n).padStart(2, "0")}.mp4`;

/** A range of 12 links, then two single links. */
const list: ParsedList = {
  links: [
    ...Array.from({ length: 12 }, (_, i) => ({ url: url(i + 1), count: 1, pattern: 0 })),
    { url: "https://a.example/slides.pdf", count: 1, pattern: null },
    { url: "https://a.example/notes.txt", count: 1, pattern: null },
  ],
  patterns: [{ stem: "lesson", count: 12 }],
  skipped: [],
};

describe("LinkPreview", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  const render = (known: Map<string, Known> = new Map()) =>
    act(() => root.render(<LinkPreview entries={entriesFrom(list, known)} />));
  const rows = (kind: string) => [...host.querySelectorAll(`li[data-kind="${kind}"]`)];
  const click = (el: Element) => act(() => (el as HTMLElement).click());

  it("marks a link already in the list with an info icon, not a tick", () => {
    render(new Map([["https://a.example/notes.txt", "listed"]]));
    const [slides, notes] = rows("link");
    expect(slides.querySelector("svg.text-success")).not.toBeNull();
    expect(notes.querySelector("svg.text-success")).toBeNull();
    expect(notes.querySelector("svg.text-g400")).not.toBeNull();
    expect(notes.textContent).toContain("Already in your list");
  });

  it("opens a range to its first links, and a click on N more lists the rest", () => {
    render();
    expect(rows("pattern-link")).toHaveLength(0);
    click(host.querySelector('button[aria-expanded="false"]')!);
    expect(rows("pattern-link")).toHaveLength(3);
    const more = rows("pattern-more")[0];
    expect(more.textContent).toBe("9 more");
    click(more.querySelector("button")!);
    expect(rows("pattern-link").map((li) => li.textContent)).toEqual(
      Array.from({ length: 12 }, (_, i) => `lesson${String(i + 1).padStart(2, "0")}.mp4`),
    );
    expect(rows("pattern-more")).toHaveLength(0);
  });

  it("gives an open range room, and closing it starts the range over", () => {
    render();
    const listBox = () => host.querySelector("ul")!;
    expect(listBox().className).toContain("max-h-40");
    click(host.querySelector('button[aria-expanded="false"]')!);
    expect(listBox().className).toContain("max-h-80");
    click(rows("pattern-more")[0].querySelector("button")!);
    click(host.querySelector('button[aria-expanded="true"]')!);
    expect(listBox().className).toContain("max-h-40");
    click(host.querySelector('button[aria-expanded="false"]')!);
    expect(rows("pattern-link")).toHaveLength(3);
  });
});
