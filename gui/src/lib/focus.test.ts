import { describe, expect, it } from "vitest";
import { focusIsFree } from "./focus";

describe("focusIsFree", () => {
  const body = {} as HTMLElement;
  it("is true when nothing or the body holds focus", () => {
    expect(focusIsFree({ activeElement: null, body })).toBe(true);
    expect(focusIsFree({ activeElement: body, body })).toBe(true);
  });
  it("is false once the user has focused something else", () => {
    expect(focusIsFree({ activeElement: {} as Element, body })).toBe(false);
  });
});
