import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AdminApp } from "./AdminApp";
import "./messages";
import { matchAdminRoute } from "./nav";
import { suggestSlug } from "./TenantsPage";

const ME = { operator: { id: "o1", username: "ops", full_name: "Người vận hành" }, step_up_until: null };
const SPA = {
  id: "0190b6a1-0000-7000-8000-000000000001",
  name: "Viên Phúc Y Đường",
  slug: "vpyd",
  status: "active",
  created_at: "2026-11-01T02:00:00Z",
  staff_count: 6,
  customer_count: 240,
  visits_30d: 310,
  owner_count: 1,
  last_activity_at: "2026-11-09T03:00:00Z",
};

const problem = (status: number, code: string, errors: unknown[] = []) =>
  new Response(JSON.stringify({ status, code, errors }), { status });

type Route = (method: string, path: string, body: unknown) => Response;

function server(route: Route) {
  return vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.pathname : input.url;
    const body: unknown = typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
    return Promise.resolve(route(init?.method ?? "GET", path, body));
  });
}

function at(path: string) {
  window.history.pushState(null, "", path);
  render(<AdminApp />);
}

afterEach(() => {
  vi.restoreAllMocks();
  window.history.pushState(null, "", "/");
});

describe("platform console routes", () => {
  it("matches only its own paths", () => {
    expect(matchAdminRoute("/admin")).toEqual({ page: "tenants" });
    expect(matchAdminRoute("/admin/")).toEqual({ page: "tenants" });
    expect(matchAdminRoute("/admin/dang-nhap")).toEqual({ page: "login" });
    expect(matchAdminRoute(`/admin/spa/${SPA.id}`)).toEqual({ page: "tenant", id: SPA.id });
    expect(matchAdminRoute("/admin/spa/not-an-id")).toEqual({ page: "not_found" });
  });

  it("suggests a slug from the spa name", () => {
    expect(suggestSlug("Spa Hoa Sen Đà Nẵng")).toBe("spa-hoa-sen-da-nang");
    expect(suggestSlug("  ")).toBe("");
  });
});

describe("operator sign-in", () => {
  it("is always two steps: password, then a code", async () => {
    let signedIn = false;
    const spy = server((method, path, body) => {
      if (path === "/api/v1/admin/me") return signedIn ? Response.json(ME) : problem(401, "auth.unauthenticated");
      if (path === "/api/v1/admin/auth/login") {
        const b = body as { username: string; password: string };
        return b.password === "dung-mat-khau" ? Response.json({ next: "mfa_verify" }) : problem(401, "auth.invalid_credentials");
      }
      if (path === "/api/v1/admin/auth/mfa/verify") {
        signedIn = true;
        return Response.json(ME.operator);
      }
      if (path === "/api/v1/admin/tenants") return Response.json([SPA]);
      return new Response(null, { status: 404 });
    });
    at("/admin");
    const user = userEvent.setup();
    expect(await screen.findByRole("heading", { name: "Đăng nhập" })).toBeInTheDocument();
    expect(window.location.pathname).toBe("/admin/dang-nhap");

    await user.type(screen.getByLabelText("Tên đăng nhập"), "ops");
    await user.type(screen.getByLabelText("Mật khẩu"), "sai");
    await user.click(screen.getByRole("button", { name: "Tiếp tục" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Tên đăng nhập hoặc mật khẩu không đúng.");

    await user.type(screen.getByLabelText("Mật khẩu"), "dung-mat-khau");
    await user.click(screen.getByRole("button", { name: "Tiếp tục" }));
    await user.type(await screen.findByLabelText("Mã xác thực", { selector: "#admin-code" }), "123456");
    await user.click(screen.getByRole("button", { name: "Xác nhận" }));
    expect(await screen.findByRole("link", { name: SPA.name })).toBeInTheDocument();
    expect(window.location.pathname).toBe("/admin");
    // The console never asks the spa's session.
    expect(spy.mock.calls.some(([input]) => typeof input === "string" && input.endsWith("/api/v1/me"))).toBe(false);
  });
});

describe("spa list", () => {
  it("shows counts only, and creating a spa shows the owner's password once after a step-up", async () => {
    let steppedUp = false;
    const created: unknown[] = [];
    server((method, path, body) => {
      if (path === "/api/v1/admin/me") return Response.json(ME);
      if (path === "/api/v1/admin/tenants" && method === "GET") return Response.json([SPA]);
      if (path === "/api/v1/admin/tenants" && method === "POST") {
        if (!steppedUp) return problem(403, "platform.step_up_required");
        created.push(body);
        return Response.json(
          {
            tenant: { ...SPA, id: "0190b6a1-0000-7000-8000-000000000002", name: "Spa Hoa Sen", slug: "spa-hoa-sen" },
            owner: { user_id: "u9", phone: "0907 000 111", temporary_password: "ABCD-EFGH-JKLM" },
          },
          { status: 201 },
        );
      }
      if (path === "/api/v1/admin/auth/reauth") {
        steppedUp = true;
        return new Response(null, { status: 204 });
      }
      return new Response(null, { status: 404 });
    });
    at("/admin");
    const user = userEvent.setup();
    const row = (await screen.findByRole("link", { name: SPA.name })).closest("tr");
    expect(row).not.toBeNull();
    expect(within(row as HTMLElement).getByText("240")).toBeInTheDocument();
    expect(within(row as HTMLElement).getByText("Đang hoạt động")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Tạo spa" }));
    const form = await screen.findByRole("dialog", { name: "Tạo spa" });
    await user.type(within(form).getByLabelText("Tên spa"), "Spa Hoa Sen");
    expect(within(form).getByLabelText("Mã spa")).toHaveValue("spa-hoa-sen");
    await user.type(within(form).getByLabelText("Tên chủ spa"), "Chủ Hoa Sen");
    await user.type(within(form).getByLabelText("Số điện thoại chủ spa"), "0907000111");
    await user.click(within(form).getByRole("button", { name: "Tạo spa" }));

    const stepUp = await screen.findByRole("dialog", { name: "Xác nhận lại" });
    await user.type(within(stepUp).getByLabelText("Mật khẩu"), "dung-mat-khau");
    await user.type(within(stepUp).getByLabelText("Mã xác thực"), "123456");
    await user.click(within(stepUp).getByRole("button", { name: "Xác nhận" }));

    expect(await screen.findByLabelText("Mật khẩu tạm")).toHaveTextContent("ABCD-EFGH-JKLM");
    expect(created).toEqual([
      {
        name: "Spa Hoa Sen",
        slug: "spa-hoa-sen",
        branch_name: null,
        owner: { full_name: "Chủ Hoa Sen", phone: "0907000111" },
      },
    ]);
    await user.click(screen.getByRole("button", { name: "Tôi đã lưu mật khẩu" }));
    await waitFor(() => {
      expect(screen.queryByLabelText("Mật khẩu tạm")).not.toBeInTheDocument();
    });
  });

  it("asks for a reason before suspending a spa", async () => {
    const calls: unknown[] = [];
    let status = "active";
    server((method, path, body) => {
      if (path === "/api/v1/admin/me") return Response.json(ME);
      if (path === `/api/v1/admin/tenants/${SPA.id}`) return Response.json({ ...SPA, status });
      if (path === `/api/v1/admin/tenants/${SPA.id}/suspend`) {
        calls.push(body);
        const reason = (body as { reason: string }).reason;
        if (reason.length < 3) return problem(422, "validation_failed", [{ field: "reason", code: "required" }]);
        status = "suspended";
        return Response.json({ tenant: { ...SPA, status }, revoked_sessions: 2 });
      }
      if (path === "/api/v1/admin/tenants") return Response.json([SPA]);
      return new Response(null, { status: 404 });
    });
    at(`/admin/spa/${SPA.id}`);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Tạm ngưng spa" }));
    const sheet = await screen.findByRole("dialog", { name: `Tạm ngưng ${SPA.name}?` });
    await user.click(within(sheet).getByRole("button", { name: "Tạm ngưng" }));
    expect(await within(sheet).findByText("Nhập lý do (ít nhất 3 ký tự).")).toBeInTheDocument();
    await user.type(within(sheet).getByLabelText("Lý do"), "Chưa gia hạn");
    await user.click(within(sheet).getByRole("button", { name: "Tạm ngưng" }));
    await waitFor(() => {
      expect(calls).toEqual([{ reason: "" }, { reason: "Chưa gia hạn" }]);
    });
    expect(await screen.findByRole("button", { name: "Mở lại spa" })).toBeInTheDocument();
  });
});

describe("feature switches", () => {
  it("turns a feature off for one spa with a reason", async () => {
    let appointments = true;
    const puts: unknown[] = [];
    server((method, path, body) => {
      if (path === "/api/v1/admin/me") return Response.json(ME);
      if (path === `/api/v1/admin/tenants/${SPA.id}`) return Response.json(SPA);
      if (path === `/api/v1/admin/tenants/${SPA.id}/features`) {
        return Response.json([
          { key: "appointments", name: "Lịch hẹn", enabled: appointments, updated_at: appointments ? null : "2026-11-09T03:00:00Z" },
          { key: "payroll", name: "Bảng lương", enabled: true, updated_at: null },
        ]);
      }
      if (path === `/api/v1/admin/tenants/${SPA.id}/features/appointments` && method === "PUT") {
        puts.push(body);
        appointments = (body as { enabled: boolean }).enabled;
        return Response.json({ key: "appointments", name: "Lịch hẹn", enabled: appointments, updated_at: "2026-11-09T03:00:00Z" });
      }
      return new Response(null, { status: 404 });
    });
    at(`/admin/spa/${SPA.id}`);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Tắt Lịch hẹn" }));
    const sheet = await screen.findByRole("dialog", { name: "Tắt Lịch hẹn?" });
    await user.type(within(sheet).getByLabelText("Lý do"), "Gói cơ bản");
    await user.click(within(sheet).getByRole("button", { name: "Tắt tính năng" }));
    expect(await screen.findByRole("button", { name: "Bật Lịch hẹn" })).toBeInTheDocument();
    expect(puts).toEqual([{ enabled: false, reason: "Gói cơ bản" }]);
  });
});

describe("console sheets and caches (review 2026-11)", () => {
  const spaServer = (counts: Record<string, number>, steppedUp = { value: true }) => {
    let status = "active";
    return server((method, path) => {
      const bump = (key: string) => {
        counts[key] = (counts[key] ?? 0) + 1;
      };
      if (path === "/api/v1/admin/me") return Response.json(ME);
      if (path === "/api/v1/admin/auth/reauth") return problem(401, "auth.invalid_credentials");
      if (path === "/api/v1/admin/tenants") return Response.json([{ ...SPA, status }]);
      if (path === `/api/v1/admin/tenants/${SPA.id}`) return Response.json({ ...SPA, status });
      if (path === `/api/v1/admin/tenants/${SPA.id}/features`) {
        bump("features");
        return Response.json([{ key: "appointments", name: "Lịch hẹn", enabled: true, updated_at: null }]);
      }
      if (path === `/api/v1/admin/tenants/${SPA.id}/roles`) {
        bump("roles");
        return Response.json([]);
      }
      if (path === "/api/v1/admin/permissions") return Response.json([]);
      if (path.startsWith(`/api/v1/admin/tenants/${SPA.id}/audit`)) {
        bump("spaAudit");
        return Response.json({ items: [], next_before: null });
      }
      if (path.startsWith("/api/v1/admin/audit")) {
        bump("audit");
        return Response.json({ items: [], next_before: null });
      }
      if (path === `/api/v1/admin/tenants/${SPA.id}/suspend` && method === "POST") {
        if (!steppedUp.value) return problem(403, "platform.step_up_required");
        status = "suspended";
        return Response.json({ tenant: { ...SPA, status }, revoked_sessions: 0 });
      }
      return new Response(null, { status: 404 });
    });
  };

  it("reopens a cancelled sheet empty: no old reason, no old error", async () => {
    spaServer({});
    at(`/admin/spa/${SPA.id}`);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Tạm ngưng spa" }));
    let sheet = await screen.findByRole("dialog", { name: `Tạm ngưng ${SPA.name}?` });
    await user.type(within(sheet).getByLabelText("Lý do"), "ab");
    await user.click(within(sheet).getByRole("button", { name: "Hủy" }));
    await user.click(screen.getByRole("button", { name: "Tạm ngưng spa" }));
    sheet = await screen.findByRole("dialog", { name: `Tạm ngưng ${SPA.name}?` });
    expect(within(sheet).getByLabelText("Lý do")).toHaveValue("");
    await user.click(within(sheet).getByRole("button", { name: "Hủy" }));

    await user.click(await screen.findByRole("button", { name: "Tắt Lịch hẹn" }));
    let feature = await screen.findByRole("dialog", { name: "Tắt Lịch hẹn?" });
    await user.type(within(feature).getByLabelText("Lý do"), "Gói cơ bản");
    await user.click(within(feature).getByRole("button", { name: "Hủy" }));
    await user.click(screen.getByRole("button", { name: "Tắt Lịch hẹn" }));
    feature = await screen.findByRole("dialog", { name: "Tắt Lịch hẹn?" });
    expect(within(feature).getByLabelText("Lý do")).toHaveValue("");
    expect(within(feature).getByLabelText("Lý do")).toHaveAttribute("maxlength", "500");
  });

  it("clears the step-up dialog after Hủy", async () => {
    spaServer({}, { value: false });
    at(`/admin/spa/${SPA.id}`);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Tạm ngưng spa" }));
    const sheet = await screen.findByRole("dialog", { name: `Tạm ngưng ${SPA.name}?` });
    await user.type(within(sheet).getByLabelText("Lý do"), "Chưa gia hạn");
    await user.click(within(sheet).getByRole("button", { name: "Tạm ngưng" }));

    let stepUp = await screen.findByRole("dialog", { name: "Xác nhận lại" });
    await user.type(within(stepUp).getByLabelText("Mật khẩu"), "sai-mat-khau");
    await user.type(within(stepUp).getByLabelText("Mã xác thực"), "123456");
    await user.click(within(stepUp).getByRole("button", { name: "Xác nhận" }));
    expect(await within(stepUp).findByRole("alert")).toBeInTheDocument();
    await user.click(within(stepUp).getByRole("button", { name: "Hủy" }));

    await user.click(within(sheet).getByRole("button", { name: "Tạm ngưng" }));
    stepUp = await screen.findByRole("dialog", { name: "Xác nhận lại" });
    expect(within(stepUp).getByLabelText("Mật khẩu")).toHaveValue("");
    expect(within(stepUp).queryByRole("alert")).not.toBeInTheDocument();
  });

  it("a write refreshes the platform log but never re-reads an open spa trail (each read is audited)", async () => {
    const counts: Record<string, number> = {};
    spaServer(counts);
    at("/admin/nhat-ky");
    const user = userEvent.setup();
    await waitFor(() => {
      expect(counts.audit).toBe(1);
    });

    await user.click(screen.getByRole("link", { name: "Spa" }));
    await user.click(await screen.findByRole("link", { name: SPA.name }));
    await user.click(await screen.findByRole("button", { name: "Xem nhật ký" }));
    await waitFor(() => {
      expect(counts.spaAudit).toBe(1);
    });

    await user.click(screen.getByRole("button", { name: "Tạm ngưng spa" }));
    const sheet = await screen.findByRole("dialog", { name: `Tạm ngưng ${SPA.name}?` });
    await user.type(within(sheet).getByLabelText("Lý do"), "Chưa gia hạn");
    await user.click(within(sheet).getByRole("button", { name: "Tạm ngưng" }));
    expect(await screen.findByRole("button", { name: "Mở lại spa" })).toBeInTheDocument();
    expect(counts).toMatchObject({ spaAudit: 1, features: 1, roles: 1 });

    await user.click(screen.getByRole("link", { name: "Nhật ký" }));
    await waitFor(() => {
      expect(counts.audit).toBe(2);
    });
  });

  it("caps the create-spa names at the server's 200 characters", async () => {
    spaServer({});
    at("/admin");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Tạo spa" }));
    const form = await screen.findByRole("dialog", { name: "Tạo spa" });
    for (const label of ["Tên spa", "Chi nhánh đầu tiên (không bắt buộc)", "Tên chủ spa"]) {
      expect(within(form).getByLabelText(label)).toHaveAttribute("maxlength", "200");
    }
  });
});
