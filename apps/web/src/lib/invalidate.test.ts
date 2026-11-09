import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { invalidateManagementViews } from "./invalidate";

const seed = (qc: QueryClient, key: unknown[]) => {
  qc.setQueryData(key, { seeded: true });
};

describe("invalidateManagementViews", () => {
  it("marks reports, overview, absent list, staff views from visits and that customer's history stale; nothing else", async () => {
    const qc = new QueryClient();
    const stale = [
      ["reports", "range", "2026-10-30", "2026-11-05", "day"],
      ["reports", "ledger", "2026-11-01", "2026-11-05"],
      ["packages", "overview", "expiring", ""],
      ["customers", "inactive", 7, "all", "all", "due", ""],
      ["customers", "history", "c1", "", "", "", ""],
      ["customers", "summary", "c1"],
      ["notes", "from=2026-11-01&to=2026-11-05"],
      // commission and served days move with visits and sales
      ["staff", "payroll", "2026-11"],
      ["staff", "attendance", "2026-11"],
      ["staff", "me", "2026-11"],
      ["staff", "me-day", "2026-11-05"],
      ["staff", "me-calendar", "2026-11-03", "2026-11-09"],
    ];
    const fresh = [
      ["customers", "history", "c2", "", "", "", ""],
      ["customers", "one", "c1"],
      ["catalog", "services"],
      // the desk's technician picker is mounted on /quay: no refetch after every save
      ["staff", "active"],
      ["staff", "profiles"],
    ];
    for (const k of [...stale, ...fresh]) seed(qc, k);
    await invalidateManagementViews(qc, "c1");
    for (const k of stale) expect(qc.getQueryState(k)?.isInvalidated, JSON.stringify(k)).toBe(true);
    for (const k of fresh) expect(qc.getQueryState(k)?.isInvalidated, JSON.stringify(k)).toBe(false);
  });
});
