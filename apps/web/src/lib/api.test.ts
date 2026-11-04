import { describe, expect, it, vi } from "vitest";
import { ApiError, api, safeNextPath } from "./api";

describe("api", () => {
  it("sends JSON with same-origin credentials and returns the body", async () => {
    const spy = vi.spyOn(globalThis, "fetch").mockResolvedValue(Response.json({ ok: true }));
    await expect(api("/thing", { method: "POST", body: { a: 1 } })).resolves.toEqual({ ok: true });
    expect(spy).toHaveBeenCalledWith(
      "/api/v1/thing",
      expect.objectContaining({ method: "POST", credentials: "same-origin", body: '{"a":1}' }),
    );
  });

  it("turns problem+json into ApiError with the stable code", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ status: 401, code: "auth.invalid_credentials" }), { status: 401 }),
    );
    await expect(api("/auth/login")).rejects.toEqual(new ApiError(401, "auth.invalid_credentials"));
  });

  it("returns null for 204", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response(null, { status: 204 }));
    await expect(api("/auth/logout", { method: "POST" })).resolves.toBeNull();
  });
});

describe("safeNextPath", () => {
  it("keeps in-app paths", () => {
    expect(safeNextPath("/quay")).toBe("/quay");
    expect(safeNextPath("/khach-hang/1?tab=lich-su")).toBe("/khach-hang/1?tab=lich-su");
  });

  it("rejects anything that could leave the app", () => {
    for (const bad of ["https://evil.example", "//evil.example", "/\\evil.example", "javascript:alert(1)", "", undefined, 42]) {
      expect(safeNextPath(bad)).toBe("/");
    }
  });
});
