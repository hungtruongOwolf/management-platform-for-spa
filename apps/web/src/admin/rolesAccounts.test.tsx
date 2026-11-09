import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AdminApp } from "./AdminApp";
import "./messages";
import { locked } from "./RolesPage";

const ME = { operator: { id: "o1", username: "ops", full_name: "Người vận hành" }, step_up_until: null };
const perm = (key: string, extra: Partial<Record<"grantable" | "owner_only" | "owner_floor", boolean>> = {}) => ({
  key,
  group: key.split(".")[0] ?? key,
  grantable: true,
  owner_only: false,
  owner_floor: false,
  feature: null,
  ...extra,
});

function server(route: (method: string, path: string, body: unknown) => Response) {
  return vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const body: unknown = typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
    return Promise.resolve(route(init?.method ?? "GET", url, body));
  });
}

afterEach(() => {
  vi.restoreAllMocks();
  window.history.pushState(null, "", "/");
});

describe("role guardrails in the editor", () => {
  it("locks what the server would refuse", () => {
    const manage = perm("staff.manage", { owner_only: true, owner_floor: true });
    expect(locked("owner", manage)).toBe("Chủ spa luôn giữ");
    expect(locked("manager", manage)).toBe("Chỉ chủ spa");
    expect(locked("technician", perm("platform.tenant.manage", { grantable: false }))).toBe("Không cấp được");
    expect(locked("manager", perm("customer.write"))).toBeNull();
  });
});

describe("accounts", () => {
  it("finds a login and resets its password, shown once", async () => {
    const actions: unknown[] = [];
    server((method, url, body) => {
      if (url.endsWith("/api/v1/admin/me")) return Response.json(ME);
      if (url.includes("/api/v1/admin/accounts?q=0905")) {
        return Response.json([
          {
            id: "u1",
            full_name: "Chủ Spa",
            phone: "0905 123 456",
            status: "active",
            locked_until: null,
            mfa_enabled: true,
            must_change_password: false,
            created_at: "2026-11-01T02:00:00Z",
            memberships: [
              { tenant_id: "t1", tenant_name: "Viên Phúc Y Đường", tenant_status: "active", status: "active", role_keys: ["owner"], is_owner: true },
            ],
          },
        ]);
      }
      if (url.endsWith("/api/v1/admin/accounts/u1/reset-password") && method === "POST") {
        actions.push(body);
        return Response.json({ temporary_password: "ABCD-EFGH-JKLM", account: {} });
      }
      return new Response(null, { status: 404 });
    });
    window.history.pushState(null, "", "/admin/tai-khoan");
    render(<AdminApp />);
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Tìm theo số điện thoại hoặc tên"), "0905");
    await user.click(screen.getByRole("button", { name: "Tìm" }));
    expect(await screen.findByText("Viên Phúc Y Đường · chủ spa")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Đặt lại mật khẩu" }));
    const sheet = await screen.findByRole("dialog", { name: "Đặt lại mật khẩu: Chủ Spa" });
    await user.type(within(sheet).getByLabelText("Lý do"), "Chủ spa quên mật khẩu");
    await user.click(within(sheet).getByRole("button", { name: "Đặt lại" }));
    expect(await screen.findByLabelText("Mật khẩu tạm")).toHaveTextContent("ABCD-EFGH-JKLM");
    expect(actions).toEqual([{ reason: "Chủ spa quên mật khẩu" }]);
  });

  it("treats a temporary lock that has run out as no lock", async () => {
    const account = (id: string, full_name: string, locked_until: string) => ({
      id,
      full_name,
      phone: `0905 000 00${id}`,
      status: "active",
      locked_until,
      mfa_enabled: false,
      must_change_password: false,
      created_at: "2026-11-01T02:00:00Z",
      memberships: [],
    });
    const hour = 3_600_000;
    server((method, url) => {
      if (url.endsWith("/api/v1/admin/me")) return Response.json(ME);
      if (url.includes("/api/v1/admin/accounts?q=")) {
        return Response.json([
          account("1", "Hết khóa", new Date(Date.now() - hour).toISOString()),
          account("2", "Đang khóa", new Date(Date.now() + hour).toISOString()),
        ]);
      }
      return new Response(null, { status: 404 });
    });
    window.history.pushState(null, "", "/admin/tai-khoan");
    render(<AdminApp />);
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Tìm theo số điện thoại hoặc tên"), "0905");
    await user.click(screen.getByRole("button", { name: "Tìm" }));
    const row = (name: string) => (screen.getByText(name).closest("li") as HTMLElement);

    await screen.findByText("Hết khóa");
    expect(within(row("Hết khóa")).queryByText(/Khóa tạm tới/)).not.toBeInTheDocument();
    expect(within(row("Hết khóa")).getByRole("button", { name: "Khóa tài khoản" })).toBeInTheDocument();
    expect(within(row("Hết khóa")).queryByRole("button", { name: "Mở khóa tài khoản" })).not.toBeInTheDocument();

    expect(within(row("Đang khóa")).getByText(/Khóa tạm tới/)).toBeInTheDocument();
    expect(within(row("Đang khóa")).getByRole("button", { name: "Mở khóa tài khoản" })).toBeInTheDocument();
    expect(within(row("Đang khóa")).queryByRole("button", { name: "Khóa tài khoản" })).not.toBeInTheDocument();
  });

  it("keeps the reason sheet's last text to itself after Hủy", async () => {
    server((method, url) => {
      if (url.endsWith("/api/v1/admin/me")) return Response.json(ME);
      if (url.includes("/api/v1/admin/accounts?q=")) {
        return Response.json([
          {
            id: "u1",
            full_name: "Chủ Spa",
            phone: "0905 123 456",
            status: "active",
            locked_until: null,
            mfa_enabled: false,
            must_change_password: false,
            created_at: "2026-11-01T02:00:00Z",
            memberships: [],
          },
        ]);
      }
      return new Response(null, { status: 404 });
    });
    window.history.pushState(null, "", "/admin/tai-khoan");
    render(<AdminApp />);
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Tìm theo số điện thoại hoặc tên"), "0905");
    await user.click(screen.getByRole("button", { name: "Tìm" }));
    await user.click(await screen.findByRole("button", { name: "Khóa tài khoản" }));
    let sheet = await screen.findByRole("dialog", { name: "Khóa tài khoản: Chủ Spa" });
    expect(within(sheet).getByLabelText("Lý do")).toHaveAttribute("maxlength", "500");
    await user.type(within(sheet).getByLabelText("Lý do"), "Nghi bị lộ mật khẩu");
    await user.click(within(sheet).getByRole("button", { name: "Hủy" }));
    await user.click(screen.getByRole("button", { name: "Đăng xuất mọi thiết bị" }));
    sheet = await screen.findByRole("dialog", { name: "Đăng xuất mọi thiết bị: Chủ Spa" });
    expect(within(sheet).getByLabelText("Lý do")).toHaveValue("");
  });
});
