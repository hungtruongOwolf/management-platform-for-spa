import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";

const ME = {
  user: { id: "u2", full_name: "Nguyễn Thị Thu Hà", phone: "0912 300 111" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions: ["catalog.read", "staff.read_own"],
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

function renderAt(path: string) {
  const history = createMemoryHistory({ initialEntries: [path] });
  render(<App history={history} />);
  return history;
}

describe("temporary password (ADR-0016)", () => {
  it("the first sign-in leads only to 'Đặt mật khẩu mới'; afterwards the app opens", async () => {
    let stage: "anonymous" | "must_change" | "done" = "anonymous";
    const spy = server((method, path, body) => {
      if (path === "/api/v1/me") {
        if (stage === "done") return Response.json(ME);
        return stage === "must_change" ? problem(403, "auth.password_change_required") : problem(401, "auth.unauthenticated");
      }
      if (path === "/api/v1/auth/login") {
        stage = "must_change";
        return Response.json({ next: "password_change", me: null });
      }
      if (path === "/api/v1/auth/password" && method === "POST") {
        const b = body as { current_password: string; new_password: string };
        if (b.current_password !== "ABCD-EFGH-JKLM") return problem(401, "auth.invalid_credentials");
        if (b.new_password === "0912300111") return problem(422, "validation_failed", [{ field: "new_password", code: "is_phone" }]);
        stage = "done";
        return Response.json(ME);
      }
      return new Response(null, { status: 404 });
    });
    const history = renderAt("/dang-nhap");
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Số điện thoại"), "0912300111");
    await user.type(screen.getByLabelText("Mật khẩu"), "ABCD-EFGH-JKLM");
    await user.click(screen.getByRole("button", { name: "Đăng nhập" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Đặt mật khẩu mới" })).toBeInTheDocument();
    expect(history.location.pathname).toBe("/doi-mat-khau");

    await user.type(screen.getByLabelText("Mật khẩu tạm"), "ABCD-EFGH-JKLM");
    await user.type(screen.getByLabelText("Mật khẩu mới"), "ngan");
    await user.type(screen.getByLabelText("Nhập lại mật khẩu mới"), "ngan");
    await user.click(screen.getByRole("button", { name: "Đổi mật khẩu" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("ít nhất 10 ký tự");

    await user.clear(screen.getByLabelText("Mật khẩu mới"));
    await user.clear(screen.getByLabelText("Nhập lại mật khẩu mới"));
    await user.type(screen.getByLabelText("Mật khẩu mới"), "0912300111");
    await user.type(screen.getByLabelText("Nhập lại mật khẩu mới"), "0912300111");
    await user.click(screen.getByRole("button", { name: "Đổi mật khẩu" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Không dùng số điện thoại");

    await user.clear(screen.getByLabelText("Mật khẩu mới"));
    await user.clear(screen.getByLabelText("Nhập lại mật khẩu mới"));
    await user.type(screen.getByLabelText("Mật khẩu mới"), "mot-cau-de-nho");
    await user.type(screen.getByLabelText("Nhập lại mật khẩu mới"), "mot-cau-de-nho");
    await user.click(screen.getByRole("button", { name: "Đổi mật khẩu" }));
    expect(await screen.findByText("Xin chào, Nguyễn Thị Thu Hà")).toBeInTheDocument();
    const calls = spy.mock.calls.filter(([input]) => input === "/api/v1/auth/password");
    expect(calls.at(-1)?.[1]?.body).toBe(JSON.stringify({ current_password: "ABCD-EFGH-JKLM", new_password: "mot-cau-de-nho" }));
  });

  it("an app screen opened during a forced change goes back to 'Đặt mật khẩu mới'", async () => {
    server((_m, path) => (path === "/api/v1/me" ? problem(403, "auth.password_change_required") : new Response(null, { status: 404 })));
    const history = renderAt("/nhan-vien");
    expect(await screen.findByRole("heading", { level: 1, name: "Đặt mật khẩu mới" })).toBeInTheDocument();
    expect(history.location.pathname).toBe("/doi-mat-khau");
  });
});

describe("Quên mật khẩu? (option A)", () => {
  it("sends the phone and always says the same, whatever the server knows", async () => {
    const spy = server((method, path) => {
      if (path === "/api/v1/me") return problem(401, "auth.unauthenticated");
      if (path === "/api/v1/auth/password-reset-requests" && method === "POST") return Response.json({ status: "received" }, { status: 202 });
      return new Response(null, { status: 404 });
    });
    renderAt("/dang-nhap");
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Số điện thoại"), "0912 300 444");
    await user.click(screen.getByRole("button", { name: "Quên mật khẩu?" }));
    const phone = await screen.findByLabelText("Số điện thoại", { selector: "#forgot-phone" });
    expect(phone).toHaveValue("0912300444");
    await user.click(screen.getByRole("button", { name: "Gửi yêu cầu" }));
    expect(await screen.findByText(/Nếu số này có tài khoản, chủ spa sẽ thấy yêu cầu/)).toBeInTheDocument();
    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith(
        "/api/v1/auth/password-reset-requests",
        expect.objectContaining({ method: "POST", body: JSON.stringify({ phone: "0912300444" }) }),
      );
    });
  });
});
