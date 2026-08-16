import { describe, expect, test } from "bun:test";
import {
  isCalendarVersion,
  nextCalendarVersion,
  parseCalendarVersion,
} from "../scripts/release/calendar-version.ts";

describe("calendar release versions", () => {
  test("migrates the final legacy version to the first dated release", () => {
    expect(nextCalendarVersion("0.2.0", "2026-08-16")).toBe("0.20260816.0");
  });

  test("increments the revision for another release on the same day", () => {
    expect(nextCalendarVersion("0.20260816.0", "2026-08-16")).toBe("0.20260816.1");
    expect(nextCalendarVersion("0.20260816.41", "2026-08-16")).toBe("0.20260816.42");
  });

  test("resets the revision on the next release day", () => {
    expect(nextCalendarVersion("0.20260816.7", "2026-08-17")).toBe("0.20260817.0");
  });

  test("rejects invalid dates, regressions, and unknown legacy lines", () => {
    expect(() => nextCalendarVersion("0.20260816.0", "2026-02-30")).toThrow();
    expect(() => nextCalendarVersion("0.20260816.0", "2026-08-15")).toThrow();
    expect(() => nextCalendarVersion("0.1.0", "2026-08-16")).toThrow();
  });

  test("parses only valid pkgshift calendar versions", () => {
    expect(parseCalendarVersion("0.20260816.3")).toEqual({
      epoch: 0,
      date: "20260816",
      revision: 3,
    });
    expect(isCalendarVersion("0.20260230.0")).toBeFalse();
    expect(isCalendarVersion("0.2.0")).toBeFalse();
  });
});
