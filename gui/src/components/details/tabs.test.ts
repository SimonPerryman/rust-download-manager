import { describe, expect, it } from "vitest";
import { effectiveTab, visibleTabs } from "./tabs";

describe("details tabs for a missing file", () => {
  it("drops Speed, and keeps all three tabs otherwise", () => {
    expect(visibleTabs(true).map((t) => t.key)).toEqual(["overview", "log"]);
    expect(visibleTabs(false).map((t) => t.key)).toEqual(["overview", "speed", "log"]);
  });

  it("falls back to Overview when the open tab is Speed", () => {
    expect(effectiveTab("speed", true)).toBe("overview");
    expect(effectiveTab("speed", false)).toBe("speed");
    expect(effectiveTab("log", true)).toBe("log");
  });
});
