import { createMemoryHistory } from "@tanstack/react-router";
import { render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";

const me = (permissions: string[]) => ({
  user: { id: "u1", full_name: "Quản lý Ngọc", phone: "0905123456" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions,
});

/** Every GET the app sends; `/customers/c1` itself never answers, like a slow network. */
function serve(permissions: string[]): string[] {
  const urls: string[] = [];
  vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    urls.push(url);
    if (url === "/api/v1/me") return Promise.resolve(Response.json(me(permissions)));
    if (url === "/api/v1/customers/c1") return new Promise<Response>(() => undefined);
    return new Promise<Response>(() => undefined);
  });
  return urls;
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("customer profile loading (perf page §2d)", () => {
  it("asks for the profile, packages, contacts, summary and history together, from the id in the URL", async () => {
    const urls = serve(["customer.read", "report.read"]);
    render(<App history={createMemoryHistory({ initialEntries: ["/khach-hang/c1"] })} />);
    await waitFor(() => {
      expect(urls).toEqual(
        expect.arrayContaining([
          "/api/v1/customers/c1",
          "/api/v1/customers/c1/packages",
          "/api/v1/customers/c1/follow-ups",
          "/api/v1/customers/c1/summary",
          expect.stringMatching(/^\/api\/v1\/customers\/c1\/history/),
        ]),
      );
    });
  });

  it("does not ask for the management panels a role cannot see", async () => {
    const urls = serve(["customer.read"]);
    render(<App history={createMemoryHistory({ initialEntries: ["/khach-hang/c1"] })} />);
    await waitFor(() => {
      expect(urls).toContain("/api/v1/customers/c1/packages");
    });
    expect(urls.filter((u) => /follow-ups|summary|history/.test(u))).toEqual([]);
  });
});
