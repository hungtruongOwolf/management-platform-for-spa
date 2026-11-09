import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { invalidateAfterFollowUp } from "./followUps";

describe("invalidateAfterFollowUp", () => {
  it("marks the absent list, that customer's contacts and summary, and the notes journal stale; nothing else", async () => {
    const qc = new QueryClient();
    const stale = [
      ["customers", "inactive", 7, "all", "all", "due", ""],
      ["customers", "follow-ups", "c1"],
      ["customers", "summary", "c1"],
      ["notes", "from=2026-11-01&to=2026-11-05"],
    ];
    const fresh = [
      ["customers", "follow-ups", "c2"],
      ["customers", "one", "c1"],
      ["reports", "range", "2026-11-01", "2026-11-05", "day"],
    ];
    for (const k of [...stale, ...fresh]) qc.setQueryData(k, { seeded: true });
    await invalidateAfterFollowUp(qc, "c1");
    for (const k of stale) expect(qc.getQueryState(k)?.isInvalidated, JSON.stringify(k)).toBe(true);
    for (const k of fresh) expect(qc.getQueryState(k)?.isInvalidated, JSON.stringify(k)).toBe(false);
  });
});
