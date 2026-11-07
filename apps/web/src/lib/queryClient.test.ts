import { describe, expect, it, vi } from "vitest";
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
});
