import { describe, expect, it, vi } from "vitest";
import { recordHistoryQuery } from "../features/record-history/query";
import { customerPackagesQuery } from "../features/sales/api";
import { payrollQuery } from "../features/staff/api";
import { DEFAULT_STALE_MS, createQueryClient } from "./queryClient";

describe("createQueryClient", () => {
  it("serves an answer from cache for 30 s, then asks again", async () => {
    const qc = createQueryClient();
    const fetcher = vi.fn().mockResolvedValue("x");
    await qc.query({ queryKey: ["k"], queryFn: fetcher });
    await qc.query({ queryKey: ["k"], queryFn: fetcher });
    expect(fetcher).toHaveBeenCalledTimes(1);
    const query = qc.getQueryCache().find({ queryKey: ["k"] });
    expect(query?.isStaleByTime(DEFAULT_STALE_MS)).toBe(false);
    vi.spyOn(Date, "now").mockReturnValue(Date.now() + DEFAULT_STALE_MS + 1);
    try {
      expect(query?.isStaleByTime(DEFAULT_STALE_MS)).toBe(true);
    } finally {
      vi.restoreAllMocks();
    }
  });

  it("refetches on focus (stale queries only) and never retries on its own", () => {
    const defaults = createQueryClient().getDefaultOptions().queries;
    expect(defaults).toMatchObject({ staleTime: DEFAULT_STALE_MS, refetchOnWindowFocus: true, retry: false });
  });

  it("package balances and payroll are not served from the 30 s cache", () => {
    expect(customerPackagesQuery("c").staleTime).toBeLessThanOrEqual(5_000);
    expect(payrollQuery("2026-11").staleTime).toBe(0);
  });

  it("any successful write marks every record's history stale, in one place (ADR-0022); a failed one does not", async () => {
    const qc = createQueryClient();
    expect(recordHistoryQuery({ type: "visit", id: "v1" }).staleTime).toBe(0);
    qc.setQueryData(["record-history", "visit", "v1"], { items: [] });
    qc.setQueryData(["visits", "today"], []);
    const history = () => qc.getQueryState(["record-history", "visit", "v1"])?.isInvalidated;
    await qc.getMutationCache().build(qc, { mutationFn: () => Promise.reject(new Error("no")) }).execute(undefined).catch(() => undefined);
    expect(history()).toBe(false);
    await qc.getMutationCache().build(qc, { mutationFn: () => Promise.resolve("ok") }).execute(undefined);
    expect(history()).toBe(true);
    // Only the histories: the screens' own lists keep their own invalidation.
    expect(qc.getQueryState(["visits", "today"])?.isInvalidated).toBe(false);
  });
});
