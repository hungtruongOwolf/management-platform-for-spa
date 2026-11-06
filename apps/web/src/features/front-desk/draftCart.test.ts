import { afterEach, describe, expect, it, vi } from "vitest";
import {
  DRAFT_MAX_AGE_MS,
  clearAllDrafts,
  clearDraft,
  loadDraft,
  saveDraft,
} from "./draftCart";

interface Draft {
  lines: { serviceId: string; staffId: string }[];
  note?: string;
}

const draft: Draft = {
  lines: [{ serviceId: "s1", staffId: "st1" }],
  note: "Đau vai gáy (sức khỏe)",
};

afterEach(() => {
  sessionStorage.clear();
  vi.restoreAllMocks();
});

describe("draft cart (Q-U8)", () => {
  it("survives a reload for the same user and customer, never for another", () => {
    saveDraft("u1", "c1", draft, 1_000);
    expect((loadDraft("u1", "c1", 2_000) as Draft | null)?.lines).toEqual(
      draft.lines,
    );
    expect(loadDraft("u2", "c1", 2_000)).toBeNull();
    expect(loadDraft("u1", "c2", 2_000)).toBeNull();
  });

  it("never stores the session note (health data)", () => {
    saveDraft("u1", "c1", draft);
    const stored = Array.from({ length: sessionStorage.length }, (_, i) =>
      sessionStorage.getItem(sessionStorage.key(i) ?? ""),
    ).join("");
    expect(stored).toContain("s1");
    expect(stored).not.toContain("Đau vai gáy");
    expect((loadDraft("u1", "c1") as Draft | null)?.note).toBeUndefined();
  });

  it("forgets a stale draft", () => {
    saveDraft("u1", "c1", draft, 0);
    expect(loadDraft("u1", "c1", DRAFT_MAX_AGE_MS + 1)).toBeNull();
    expect(sessionStorage.length).toBe(0);
  });

  it("is cleared on save and on sign-out", () => {
    saveDraft("u1", "c1", draft);
    saveDraft("u1", "c2", draft);
    sessionStorage.setItem("other", "kept");
    clearDraft("u1", "c1");
    expect(loadDraft("u1", "c1")).toBeNull();
    clearAllDrafts();
    expect(loadDraft("u1", "c2")).toBeNull();
    expect(sessionStorage.getItem("other")).toBe("kept");
  });

  it("works without storage (private window)", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(() => {
      saveDraft("u1", "c1", draft);
    }).not.toThrow();
    expect(loadDraft("u1", "c1")).toBeNull();
  });
});
