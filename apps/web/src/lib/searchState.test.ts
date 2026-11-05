import { describe, expect, it } from "vitest";
import { inactiveSearch, packagesSearch, reportsSearch } from "../features/reports/search";
import { customersSearch } from "../features/customers/search";
import { param, searchSchema } from "./searchState";

describe("search schemas", () => {
  const schema = searchSchema({
    tab: param.oneOf(["a", "b"], "a"),
    n: param.oneOfNumber([7, 14], 7),
    q: param.text(5),
    page: param.int(1, 1, 99),
    day: param.day(),
  });

  it("fills defaults for missing keys", () => {
    expect(schema.parse({})).toEqual({ tab: "a", n: 7, q: "", page: 1, day: "" });
    expect(schema.defaults).toEqual(schema.parse({}));
  });

  it("treats the URL as untrusted: bad values fall back, text is bounded", () => {
    expect(schema.parse({ tab: "x", n: 99, q: "abcdefgh", page: "0", day: "2026-13-45" })).toEqual({
      tab: "a",
      n: 7,
      q: "abcde",
      page: 1,
      day: "",
    });
    expect(schema.parse({ tab: { evil: true }, n: "14", page: "3", day: "2026-11-05" })).toMatchObject({
      tab: "a",
      n: 14,
      page: 3,
      day: "2026-11-05",
    });
  });

  it("validate keeps only valid, non-default keys (clean URLs, optional Link search)", () => {
    expect(schema.validate({ tab: "a", n: "14", q: "", junk: "1" })).toEqual({ n: 14 });
    expect(schema.compact({ ...schema.defaults, tab: "b" })).toEqual({ tab: "b" });
  });

  it("the app's screens declare their list state", () => {
    expect(packagesSearch.defaults).toEqual({ status: "expiring" });
    expect(packagesSearch.validate({ status: "cancelled" })).toEqual({ status: "cancelled" });
    expect(inactiveSearch.defaults).toEqual({ days: 7, scope: "with_sessions" });
    expect(inactiveSearch.validate({ days: "30", scope: "all" })).toEqual({ days: 30, scope: "all" });
    expect(reportsSearch.validate({ date: "not-a-date" })).toEqual({});
    expect(customersSearch.validate({ q: "Hà" })).toEqual({ q: "Hà" });
  });
});
