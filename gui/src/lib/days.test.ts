import { describe, expect, it } from "vitest";
import { checkDays, daysKind, daysName, daysPhrase, NO_DAYS, toggled, WEEKDAYS } from "./days";

describe("days name themselves", () => {
  it("names the common sets", () => {
    expect(daysName(WEEKDAYS)).toBe("Daily");
    expect(daysKind(WEEKDAYS)).toBe("daily");
    expect(daysName(["fri", "mon", "wed", "tue", "thu"])).toBe("Weekdays");
    expect(daysName(["sun", "sat"])).toBe("Weekends");
  });

  it("lists any other set, Monday first", () => {
    expect(daysName(["thu", "mon", "tue"])).toBe("Mon, Tue and Thu");
    expect(daysName(["mon", "fri"])).toBe("Mon and Fri");
    expect(daysName(["wed"])).toBe("Wed");
    expect(daysName(["mon", "tue", "wed", "thu", "fri", "sat"])).toBe(
      "Mon, Tue, Wed, Thu, Fri and Sat",
    );
    expect(daysKind(["mon", "fri"])).toBe("chosen");
  });

  it("reads in a sentence", () => {
    expect(daysPhrase(WEEKDAYS)).toBe("every day");
    expect(daysPhrase(["mon", "tue", "wed", "thu", "fri"])).toBe("on weekdays");
    expect(daysPhrase(["sat", "sun"])).toBe("on weekends");
    expect(daysPhrase(["mon", "tue", "thu"])).toBe("on Mon, Tue and Thu");
  });

  it("refuses an empty set", () => {
    expect(checkDays([])).toEqual({ ok: false, error: NO_DAYS });
    expect(checkDays(["sun"])).toEqual({ ok: true });
  });

  it("toggles a day in and out, keeping the order", () => {
    expect(toggled(["mon", "wed"], "tue")).toEqual(["mon", "tue", "wed"]);
    expect(toggled(["mon", "wed"], "mon")).toEqual(["wed"]);
    expect(toggled(["wed"], "wed")).toEqual([]);
  });
});
