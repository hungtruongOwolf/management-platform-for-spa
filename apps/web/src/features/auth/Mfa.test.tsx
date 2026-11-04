import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";

const ME = {
  user: { id: "u1", full_name: "Chủ Spa", phone: "0905 000 001" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions: ["catalog.read"],
};

const problem = (status: number, code: string) => new Response(JSON.stringify({ status, code }), { status });

type Route = (method: string, path: string, body: unknown) => Response;

function server(route: Route) {
  return vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.pathname : input.url;
    const body: unknown = typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
    return Promise.resolve(route(init?.method ?? "GET", path, body));
  });
}

function renderAt(path: string) {
  return render(<App history={createMemoryHistory({ initialEntries: [path] })} />);
}

async function signIn() {
  const user = userEvent.setup();
  await user.type(await screen.findByLabelText("Số điện thoại"), "0905000001");
  await user.type(screen.getByLabelText("Mật khẩu"), "mat-khau-dung");
  await user.click(screen.getByRole("button", { name: "Đăng nhập" }));
  return user;
}

describe("two-step login", () => {
  it("owner with MFA enters a code after the password, then reaches the app", async () => {
    let stage: "anonymous" | "pending" | "done" = "anonymous";
    const fetchSpy = server((method, path, body) => {
      if (path === "/api/v1/me") {
        if (stage === "done") return Response.json(ME);
        return problem(401, stage === "pending" ? "auth.mfa_required" : "auth.unauthenticated");
      }
      if (path === "/api/v1/auth/login") {
        stage = "pending";
        return Response.json({ next: "mfa_verify", me: null });
      }
      if (path === "/api/v1/auth/mfa/verify" && method === "POST") {
        if ((body as { code: string }).code !== "123456") return problem(401, "auth.invalid_mfa_code");
        stage = "done";
        return Response.json(ME);
      }
      return new Response(null, { status: 404 });
    });
    renderAt("/dang-nhap");
    const user = await signIn();

    const code = await screen.findByLabelText("Mã xác thực (6 số)");
    await user.type(code, "999999");
    await user.click(screen.getByRole("button", { name: "Xác nhận" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Mã không đúng");
    expect(screen.getByLabelText("Mã xác thực (6 số)")).toHaveValue("");

    await user.type(screen.getByLabelText("Mã xác thực (6 số)"), "123456");
    await user.click(screen.getByRole("button", { name: "Xác nhận" }));
    expect(await screen.findByText("Xin chào, Chủ Spa")).toBeInTheDocument();
    expect(fetchSpy).toHaveBeenCalledWith(
      "/api/v1/auth/mfa/verify",
      expect.objectContaining({ method: "POST", body: JSON.stringify({ code: "123456" }) }),
    );
  });

  it("a recovery code can be used instead of the app code", async () => {
    let done = false;
    server((_method, path, body) => {
      if (path === "/api/v1/me") return done ? Response.json(ME) : problem(401, "auth.mfa_required");
      if (path === "/api/v1/auth/mfa/verify") {
        done = (body as { code: string }).code === "abcd-efgh-2345-6789";
        return done ? Response.json(ME) : problem(401, "auth.invalid_mfa_code");
      }
      return new Response(null, { status: 404 });
    });
    renderAt("/xac-thuc");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Mất điện thoại? Dùng mã khôi phục" }));
    await user.type(screen.getByLabelText("Mã khôi phục"), "abcd-efgh-2345-6789");
    await user.click(screen.getByRole("button", { name: "Xác nhận" }));
    expect(await screen.findByText("Xin chào, Chủ Spa")).toBeInTheDocument();
  });

  it("a pending-MFA session opening an app page is sent to the MFA step (UX guard)", async () => {
    server((_method, path) =>
      path === "/api/v1/me" ? problem(401, "auth.mfa_required") : new Response(null, { status: 404 }),
    );
    renderAt("/bang-gia");
    expect(await screen.findByRole("heading", { name: "Xác thực hai bước" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Bảng giá dịch vụ" })).not.toBeInTheDocument();
  });

  it("owner without MFA enrolls: QR + manual key, then recovery codes shown once", async () => {
    let stage: "pending" | "done" = "pending";
    const codes = Array.from({ length: 10 }, (_, i) => `aaaa-bbbb-cccc-${String(1000 + i)}`);
    const fetchSpy = server((method, path, body) => {
      if (path === "/api/v1/me")
        return stage === "done" ? Response.json(ME) : problem(401, "auth.mfa_enrollment_required");
      if (path === "/api/v1/auth/mfa/enroll/start" && method === "POST")
        return Response.json({
          otpauth_uri: "otpauth://totp/Vi%C3%AAn:0905%20000%20001?secret=JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP&issuer=Vi%C3%AAn",
          manual_key: "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP",
        });
      if (path === "/api/v1/auth/mfa/enroll/confirm" && method === "POST") {
        if ((body as { code: string }).code !== "654321") return problem(401, "auth.invalid_mfa_code");
        stage = "done";
        return Response.json({ me: ME, recovery_codes: codes });
      }
      return new Response(null, { status: 404 });
    });
    renderAt("/");
    expect(await screen.findByRole("heading", { name: "Thiết lập xác thực hai bước" })).toBeInTheDocument();
    expect(await screen.findByRole("img", { name: /Mã QR/ })).toBeInTheDocument();
    expect(screen.getByTestId("manual-key")).toHaveTextContent("JBSW Y3DP EHPK 3PXP JBSW Y3DP EHPK 3PXP");

    const user = userEvent.setup();
    await user.type(screen.getByLabelText("Mã xác thực (6 số)"), "654321");
    await user.click(screen.getByRole("button", { name: "Bật xác thực hai bước" }));

    expect(await screen.findByRole("heading", { name: "Mã khôi phục" })).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(10);
    const proceed = screen.getByRole("button", { name: "Tiếp tục" });
    expect(proceed).toBeDisabled();
    await user.click(screen.getByLabelText("Tôi đã lưu các mã khôi phục"));
    await user.click(proceed);
    expect(await screen.findByText("Xin chào, Chủ Spa")).toBeInTheDocument();

    const starts = fetchSpy.mock.calls.filter(([input]) => input === "/api/v1/auth/mfa/enroll/start");
    expect(starts).toHaveLength(1);
  });

  it("an expired pending session goes back to the login page", async () => {
    let expired = false;
    server((_method, path) => {
      if (path === "/api/v1/me") return problem(401, expired ? "auth.unauthenticated" : "auth.mfa_required");
      if (path === "/api/v1/auth/mfa/verify") {
        expired = true;
        return problem(401, "auth.unauthenticated");
      }
      if (path === "/api/v1/auth/logout") return new Response(null, { status: 204 });
      return new Response(null, { status: 404 });
    });
    renderAt("/xac-thuc");
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Mã xác thực (6 số)"), "123456");
    await user.click(screen.getByRole("button", { name: "Xác nhận" }));
    expect(await screen.findByRole("button", { name: "Đăng nhập" })).toBeInTheDocument();
  });
});
