import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "../../lib/api";
import { ME_RETRY_DELAYS_MS, authQuery, authRetry, confirmMfaEnrollment, login, parseMe } from "./session";

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

describe("/me under a busy API (429)", () => {
  const busy = (headers: Record<string, string> = {}) =>
    new Response(JSON.stringify({ code: "rate_limited" }), { status: 429, headers });

  function load() {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    return client.query(authQuery);
  }

  it("retries with backoff, says it is retrying, then signs in", async () => {
    vi.useFakeTimers();
    try {
      const fetch = vi
        .spyOn(globalThis, "fetch")
        .mockResolvedValueOnce(busy())
        .mockResolvedValueOnce(busy())
        .mockResolvedValue(Response.json(ME));
      const seen: boolean[] = [];
      const stop = authRetry.subscribe(() => seen.push(authRetry.get()));
      const result = load();
      await vi.advanceTimersByTimeAsync(ME_RETRY_DELAYS_MS[0] + ME_RETRY_DELAYS_MS[1]);
      await expect(result).resolves.toEqual({ state: "signed_in", me: ME });
      stop();
      expect(fetch).toHaveBeenCalledTimes(3);
      expect(seen).toEqual([true, false]);
      expect(authRetry.get()).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("waits the server's Retry-After instead of the default step", async () => {
    vi.useFakeTimers();
    try {
      const fetch = vi
        .spyOn(globalThis, "fetch")
        .mockResolvedValueOnce(busy({ "Retry-After": "3" }))
        .mockResolvedValue(Response.json(ME));
      const result = load();
      await vi.advanceTimersByTimeAsync(2999);
      expect(fetch).toHaveBeenCalledTimes(1);
      await vi.advanceTimersByTimeAsync(1);
      await expect(result).resolves.toEqual({ state: "signed_in", me: ME });
      expect(fetch).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it("gives up after the last step and reports the 429 (the error page, as before)", async () => {
    vi.useFakeTimers();
    try {
      const fetch = vi.spyOn(globalThis, "fetch").mockImplementation(() => Promise.resolve(busy()));
      const result = load();
      const settled = expect(result).rejects.toMatchObject({ status: 429 });
      await vi.advanceTimersByTimeAsync(ME_RETRY_DELAYS_MS.reduce((a, b) => a + b, 0));
      await settled;
      expect(fetch).toHaveBeenCalledTimes(ME_RETRY_DELAYS_MS.length + 1);
      expect(authRetry.get()).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not retry a real answer (401 = signed out)", async () => {
    const fetch = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(new Response(JSON.stringify({ code: "auth.unauthenticated" }), { status: 401 }));
    await expect(load()).resolves.toEqual({ state: "anonymous" });
    expect(fetch).toHaveBeenCalledTimes(1);
  });
});
