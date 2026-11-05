import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "./App";

const ME = {
  user: { id: "u1", full_name: "Chủ Spa", phone: "0905 123 456" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions: ["catalog.read"],
};

type Route = (method: string, path: string, body: unknown) => Response;

function server(route: Route) {
  return vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const path =
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.pathname
          : input.url;
    const method = init?.method ?? "GET";
    const body: unknown =
      typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
    return Promise.resolve(route(method, path, body));
  });
}

const unauthorized = () =>
  new Response(JSON.stringify({ status: 401, code: "auth.unauthenticated" }), {
    status: 401,
  });

function renderAt(path: string) {
  return render(
    <App history={createMemoryHistory({ initialEntries: [path] })} />,
  );
}

describe("auth flow", () => {
  it("sends anonymous visitors to the login page", async () => {
    server(() => unauthorized());
    renderAt("/");
    expect(
      await screen.findByRole("heading", { name: "Viên Phúc Y Đường" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Đăng nhập" }),
    ).toBeInTheDocument();
  });

  it("logs in and lands on the signed-in shell", async () => {
    let loggedIn = false;
    const fetchSpy = server((method, path, body) => {
      if (path === "/api/v1/me")
        return loggedIn ? Response.json(ME) : unauthorized();
      if (path === "/api/v1/auth/login" && method === "POST") {
        expect(body).toEqual({
          phone: "0905 123 456",
          password: "mat-khau-dung",
        });
        loggedIn = true;
        return Response.json({ next: "done", me: ME });
      }
      return new Response(null, { status: 404 });
    });
    renderAt("/");
    const user = userEvent.setup();
    await user.type(
      await screen.findByLabelText("Số điện thoại"),
      "0905 123 456",
    );
    await user.type(screen.getByLabelText("Mật khẩu"), "mat-khau-dung");
    await user.click(screen.getByRole("button", { name: "Đăng nhập" }));

    expect(await screen.findByText("Xin chào, Chủ Spa")).toBeInTheDocument();
    expect(fetchSpy).toHaveBeenCalledWith(
      "/api/v1/auth/login",
      expect.objectContaining({ credentials: "same-origin" }),
    );
  });

  it("shows a Vietnamese error and clears the password on wrong credentials", async () => {
    server((_method, path) =>
      path === "/api/v1/auth/login"
        ? new Response(
            JSON.stringify({ status: 401, code: "auth.invalid_credentials" }),
            { status: 401 },
          )
        : unauthorized(),
    );
    renderAt("/dang-nhap");
    const user = userEvent.setup();
    await user.type(
      await screen.findByLabelText("Số điện thoại"),
      "0905123456",
    );
    await user.type(screen.getByLabelText("Mật khẩu"), "sai-mat-khau");
    await user.click(screen.getByRole("button", { name: "Đăng nhập" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Số điện thoại hoặc mật khẩu không đúng.",
    );
    expect(screen.getByLabelText("Mật khẩu")).toHaveValue("");
  });

  it("validates empty fields before calling the API", async () => {
    const fetchSpy = server(() => unauthorized());
    renderAt("/dang-nhap");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Đăng nhập" }));
    expect(await screen.findByText("Nhập số điện thoại.")).toBeInTheDocument();
    expect(fetchSpy).not.toHaveBeenCalledWith(
      "/api/v1/auth/login",
      expect.anything(),
    );
  });

  it("focuses the first invalid field and clears its error once corrected", async () => {
    server(() => unauthorized());
    renderAt("/dang-nhap");
    const user = userEvent.setup();
    const phone = await screen.findByLabelText("Số điện thoại");
    await user.type(phone, "0905123456");
    await user.click(screen.getByRole("button", { name: "Đăng nhập" }));

    const password = screen.getByLabelText("Mật khẩu");
    expect(await screen.findByText("Nhập mật khẩu.")).toBeInTheDocument();
    expect(screen.queryByText("Nhập số điện thoại.")).not.toBeInTheDocument();
    expect(password).toHaveFocus();
    expect(password).toHaveAttribute("aria-invalid", "true");

    await user.type(password, "x");
    expect(screen.queryByText("Nhập mật khẩu.")).not.toBeInTheDocument();
    expect(password).not.toHaveAttribute("aria-invalid");
  });

  it("sends the phone without surrounding spaces", async () => {
    const fetchSpy = server((_method, path) =>
      path === "/api/v1/auth/login"
        ? new Response(
            JSON.stringify({ status: 401, code: "auth.invalid_credentials" }),
            { status: 401 },
          )
        : unauthorized(),
    );
    renderAt("/dang-nhap");
    const user = userEvent.setup();
    await user.type(
      await screen.findByLabelText("Số điện thoại"),
      "  0905123456 ",
    );
    await user.type(screen.getByLabelText("Mật khẩu"), "pw");
    await user.click(screen.getByRole("button", { name: "Đăng nhập" }));

    await screen.findByRole("alert");
    expect(fetchSpy).toHaveBeenCalledWith(
      "/api/v1/auth/login",
      expect.objectContaining({
        body: JSON.stringify({ phone: "0905123456", password: "pw" }),
      }),
    );
  });

  it("an already signed-in user opening the login link goes straight to `next`", async () => {
    server((_method, path) => {
      if (path === "/api/v1/me") return Response.json(ME);
      if (path === "/api/v1/service-categories" || path === "/api/v1/services")
        return Response.json([]);
      return new Response(null, { status: 404 });
    });
    renderAt("/dang-nhap?next=%2Fbang-gia");
    expect(
      await screen.findByRole("heading", { name: "Bảng giá dịch vụ" }),
    ).toBeInTheDocument();
  });

  it("quick lock logs out and returns to the login page", async () => {
    let loggedIn = true;
    const fetchSpy = server((method, path) => {
      if (path === "/api/v1/me")
        return loggedIn ? Response.json(ME) : unauthorized();
      if (path === "/api/v1/auth/logout" && method === "POST") {
        loggedIn = false;
        return new Response(null, { status: 204 });
      }
      return new Response(null, { status: 404 });
    });
    renderAt("/");
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Khóa nhanh" }));
    expect(
      await screen.findByRole("button", { name: "Đăng nhập" }),
    ).toBeInTheDocument();
    expect(fetchSpy).toHaveBeenCalledWith(
      "/api/v1/auth/logout",
      expect.objectContaining({ method: "POST" }),
    );
  });
});

describe("home page", () => {
  it("offers a shortcut only to the screens the user's permissions open", async () => {
    server((_method, path) =>
      path === "/api/v1/me"
        ? Response.json({
            ...ME,
            permissions: ["catalog.read", "customer.read", "visit.record"],
          })
        : new Response(null, { status: 404 }),
    );
    renderAt("/");
    const shortcuts = await screen.findByRole("navigation", {
      name: "Lối tắt",
    });
    const links = within(shortcuts).getAllByRole("link");
    expect(links.map((l) => l.getAttribute("href"))).toEqual([
      "/quay",
      "/khach-hang",
      "/bang-gia",
      "/bang-gia/goi",
    ]);
    // Paper-book data goes through the operator CLI (EM 2026-11-05): never a shortcut.
    expect(
      within(shortcuts).queryByRole("link", { name: /Chuyển sổ giấy/ }),
    ).toBeNull();
    expect(
      screen.getByRole("heading", { level: 1, name: "Xin chào, Chủ Spa" }),
    ).toBeInTheDocument();
  });
});
