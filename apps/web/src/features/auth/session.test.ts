import { describe, expect, it, vi } from "vitest";
import { ApiError } from "../../lib/api";
import { confirmMfaEnrollment, login, parseMe } from "./session";

const ME = {
  user: { id: "u1", full_name: "Chủ Spa", phone: "0905123456" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions: ["catalog.read"],
};

function reply(body: unknown) {
  vi.spyOn(globalThis, "fetch").mockResolvedValue(Response.json(body));
}

describe("parseMe", () => {
  it("accepts a contract-shaped payload and drops unknown fields", () => {
    expect(parseMe({ ...ME, extra: true, user: { ...ME.user, role: "x" } })).toEqual(ME);
  });

  it("accepts a user without an active spa", () => {
    expect(parseMe({ ...ME, active_tenant: null }).active_tenant).toBeNull();
  });

  it.each([
    ["not an object", "me"],
    ["an array", [ME]],
    ["missing user", { ...ME, user: undefined }],
    ["a numeric name", { ...ME, user: { ...ME.user, full_name: 1 } }],
    ["missing active_tenant", { ...ME, active_tenant: undefined }],
    ["tenants not a list", { ...ME, tenants: {} }],
    ["a bad tenant", { ...ME, tenants: [{ id: "t1" }] }],
    ["a non-string permission", { ...ME, permissions: ["a", 2] }],
  ])("rejects %s", (_label, value) => {
    expect(() => parseMe(value)).toThrow(ApiError);
  });
});

describe("login", () => {
  it("signs in when the password step is enough", async () => {
    reply({ next: "done", me: ME });
    await expect(login("0905123456", "pw")).resolves.toEqual({ state: "signed_in", me: ME });
  });

  it("reports the MFA step still owed", async () => {
    reply({ next: "mfa_enroll", me: null });
    await expect(login("0905123456", "pw")).resolves.toEqual({ state: "mfa_enroll" });
  });

  it("rejects an unknown next step or a missing user", async () => {
    reply({ next: "later" });
    await expect(login("0905123456", "pw")).rejects.toThrow(ApiError);
    reply({ next: "done", me: null });
    await expect(login("0905123456", "pw")).rejects.toThrow(ApiError);
  });
});

describe("confirmMfaEnrollment", () => {
  it("returns the user and the recovery codes", async () => {
    reply({ me: ME, recovery_codes: ["a-b", "c-d"] });
    await expect(confirmMfaEnrollment("123456")).resolves.toEqual({ me: ME, recoveryCodes: ["a-b", "c-d"] });
  });
});
