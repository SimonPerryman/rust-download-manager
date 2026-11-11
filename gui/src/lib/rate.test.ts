import { describe, expect, it } from "vitest";
import {
  NOT_A_SPEED,
  NO_LIMIT,
  ZERO_RATE,
  composeRate,
  exceeds,
  limitText,
  rateDraft,
  sameRate,
  sanitizeAmount,
  speedIn,
} from "./rate";

const KIB = 1024;
const MIB = 1024 * 1024;

describe("sanitizeAmount", () => {
  it("keeps digits and the first decimal point", () => {
    expect(sanitizeAmount("1.5")).toBe("1.5");
    expect(sanitizeAmount("1.2.3")).toBe("1.23");
  });

  it("drops anything else", () => {
    expect(sanitizeAmount("5X")).toBe("5");
    expect(sanitizeAmount("-2 M")).toBe("2");
    expect(sanitizeAmount("abc")).toBe("");
  });
});

describe("rateDraft", () => {
  it("is empty for unlimited", () => {
    expect(rateDraft(null)).toEqual(NO_LIMIT);
    expect(rateDraft(0)).toEqual(NO_LIMIT);
  });

  it("uses MB/s from 1 MiB/s up, KB/s below", () => {
    expect(rateDraft(2 * MIB)).toEqual({ amount: "2", unit: "M" });
    expect(rateDraft(1.5 * MIB)).toEqual({ amount: "1.5", unit: "M" });
    expect(rateDraft(512 * KIB)).toEqual({ amount: "512", unit: "K" });
  });

  it("rounds a rate that isn't a whole unit to two decimals", () => {
    expect(rateDraft(1000)).toEqual({ amount: "0.98", unit: "K" });
  });

  it("never shows a tiny set limit as zero", () => {
    expect(rateDraft(1)).toEqual({ amount: "0.00098", unit: "K" });
  });
});

describe("composeRate", () => {
  it("joins amount and unit into the engine's rate grammar", () => {
    expect(composeRate({ amount: "500", unit: "K" })).toEqual({ ok: true, rate: "500K" });
    expect(composeRate({ amount: "1.5", unit: "M" })).toEqual({ ok: true, rate: "1.5M" });
  });

  it("sends an empty amount as unlimited", () => {
    expect(composeRate({ amount: "", unit: "M" })).toEqual({ ok: true, rate: "" });
  });

  it("tidies a half-typed decimal the engine would reject", () => {
    expect(composeRate({ amount: "2.", unit: "M" })).toEqual({ ok: true, rate: "2M" });
    expect(composeRate({ amount: ".5", unit: "M" })).toEqual({ ok: true, rate: "0.5M" });
  });

  it("rejects a lone decimal point rather than reading it as 'no limit'", () => {
    expect(composeRate({ amount: ".", unit: "K" })).toEqual({ ok: false, error: NOT_A_SPEED });
  });

  it("rejects zero with a hint about how to mean 'no limit'", () => {
    expect(composeRate({ amount: "0", unit: "K" })).toEqual({ ok: false, error: ZERO_RATE });
    expect(composeRate({ amount: "0.0", unit: "M" })).toEqual({ ok: false, error: ZERO_RATE });
  });
});

describe("sameRate", () => {
  it("compares amount and unit", () => {
    expect(sameRate({ amount: "2", unit: "M" }, { amount: "2", unit: "M" })).toBe(true);
    expect(sameRate({ amount: "2", unit: "M" }, { amount: "2", unit: "K" })).toBe(false);
  });

  it("treats every empty amount as the same 'no limit'", () => {
    expect(sameRate({ amount: "", unit: "M" }, NO_LIMIT)).toBe(true);
  });
});

describe("exceeds", () => {
  it("is true only above a set cap", () => {
    expect(exceeds({ amount: "5", unit: "M" }, 2 * MIB)).toBe(true);
    expect(exceeds({ amount: "2", unit: "M" }, 2 * MIB)).toBe(false);
    expect(exceeds({ amount: "5", unit: "M" }, null)).toBe(false);
    expect(exceeds(NO_LIMIT, 2 * MIB)).toBe(false);
  });
});

describe("limitText", () => {
  it("reads like the editor's amount and unit, in full or short", () => {
    expect(limitText(50 * KIB)).toBe("50 KB/s");
    expect(limitText(50 * KIB, true)).toBe("50K");
    expect(limitText(1.5 * MIB)).toBe("1.5 MB/s");
    expect(limitText(1.5 * MIB, true)).toBe("1.5M");
  });
});

describe("speedIn", () => {
  it("is a bare number in the limit's unit, with fewer decimals as it grows", () => {
    expect(speedIn(49.8 * KIB, "K")).toBe("49.8");
    expect(speedIn(980, "K")).toBe("0.96");
    expect(speedIn(512.4 * KIB, "K")).toBe("512");
    expect(speedIn(1.5 * MIB, "M")).toBe("1.50");
    expect(speedIn(5, "K")).toBe("<0.01");
  });
});
