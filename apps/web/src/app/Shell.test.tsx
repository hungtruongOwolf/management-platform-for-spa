import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Me } from "../features/auth/session";
import { App } from "./App";
import { fold } from "./CommandPalette";
import { canSeeManagement, managementDataFrom } from "./features";
import { navFor, tabsFor } from "./nav";
import { applyTheme, readSidebarCollapsed, readTheme, saveSidebarCollapsed } from "./prefs";

const me = (permissions: string[]): Me => ({
  user: { id: "u1", full_name: "Nguyễn Thị Thu Hà", phone: "0905 123 456" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions,
});
const MANAGER = ["visit.record", "customer.read", "catalog.read", "sale.read"];

function serve(permissions: string[], customers: unknown[] = []) {
  return vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (url === "/api/v1/me") return Promise.resolve(Response.json(me(permissions)));
    if (url.startsWith("/api/v1/customers?q=")) return Promise.resolve(Response.json(customers));
    return Promise.resolve(Response.json([]));
  });
}

const at = (path: string) => render(<App history={createMemoryHistory({ initialEntries: [path] })} />);

afterEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset["theme"];
});

describe("navigation model", () => {
  const paths = (permissions: string[]) => navFor(me(permissions)).flatMap((g) => g.items.map((i) => i.to));

  it("lists only what the permissions open, grouped like the paper books", () => {
    expect(navFor(me(MANAGER)).map((g) => g.label)).toEqual([
      "Quầy",
      "Khách hàng",
      "Gói liệu trình",
      "Báo cáo",
      "Danh mục",
      "Cài đặt",
    ]);
    expect(paths(["catalog.read"])).toEqual(["/", "/bang-gia", "/bang-gia/goi", "/cai-dat"]);
  });

  it("never lists the paper-book import (EM 2026-11-05)", () => {
    expect(paths([...MANAGER, "opening_balance.import", "package.cancel_refund"])).not.toContain("/nhap-so-cu");
  });

  it("keeps the phone tab bar to the four main screens", () => {
    expect(tabsFor(navFor(me(MANAGER))).map((t) => t.to)).toEqual(["/", "/quay", "/khach-hang", "/bao-cao"]);
    expect(tabsFor(navFor(me(["catalog.read"]))).map((t) => t.to)).toEqual(["/"]);
  });
});

describe("management views flag", () => {
  it("is off in production builds unless asked for, mock in development", () => {
    expect(managementDataFrom(undefined, false)).toBe("off");
    expect(managementDataFrom(undefined, true)).toBe("mock");
    expect(managementDataFrom("api", true)).toBe("api");
    expect(managementDataFrom("nonsense", false)).toBe("off");
  });

  it("gates on report.read with the real API and on sale.read with the mock", () => {
    expect(canSeeManagement(me(["sale.read"]), "off")).toBe(false);
    expect(canSeeManagement(me(["sale.read"]), "mock")).toBe(true);
    expect(canSeeManagement(me(["sale.read"]), "api")).toBe(false);
    expect(canSeeManagement(me(["report.read"]), "api")).toBe(true);
    expect(canSeeManagement(null, "mock")).toBe(false);
  });
});

describe("device preferences", () => {
  it("applies and remembers the theme; 'system' clears the override", () => {
    applyTheme("dark");
    expect(document.documentElement.dataset["theme"]).toBe("dark");
    expect(readTheme()).toBe("dark");
    applyTheme("system");
    expect(document.documentElement.dataset["theme"]).toBeUndefined();
    expect(readTheme()).toBe("system");
  });

  it("falls back to defaults when storage throws (private mode)", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(readTheme()).toBe("system");
    expect(() => {
      applyTheme("light");
      saveSidebarCollapsed(true);
    }).not.toThrow();
    expect(readSidebarCollapsed()).toBe(false);
  });
});

describe("command palette", () => {
  it("matches without diacritics", () => {
    expect(fold("Hồ sơ khách Đường")).toBe("ho so khach duong");
  });

  it("opens with Ctrl+K, finds a customer and opens the profile", { timeout: 20_000 }, async () => {
    serve(MANAGER, [{ id: "c9", full_name: "Phan Thị Thùy Dung", phone: "0399 000 002", gender: "female" }]);
    const user = userEvent.setup();
    at("/bang-gia");
    await screen.findByRole("navigation", { name: "Chính" });
    await user.keyboard("{Control>}k{/Control}");
    const box = await screen.findByRole("combobox", { name: "Tìm khách hoặc màn hình" });
    await user.type(box, "dung");
    const people = await screen.findByRole("group", { name: "Khách hàng" });
    expect(await within(people).findByRole("option", { name: /Phan Thị Thùy Dung/ })).toBeInTheDocument();
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("heading", { level: 1 })).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Tìm khách hoặc màn hình" })).not.toBeInTheDocument();
  });

  it("jumps to a screen by name and only offers screens the user may open", async () => {
    serve(["catalog.read"]);
    const user = userEvent.setup();
    at("/");
    await user.click(await screen.findByRole("button", { name: /Tìm khách hoặc mở màn hình/ }));
    const box = await screen.findByRole("combobox", { name: "Tìm khách hoặc màn hình" });
    await user.type(box, "goi ban");
    const screens = await screen.findByRole("group", { name: "Màn hình" });
    expect(within(screens).getAllByRole("option").map((o) => o.textContent)).toEqual([
      expect.stringContaining("Gói bán"),
    ]);
    await user.clear(box);
    await user.type(box, "quay");
    expect(screen.queryByRole("option", { name: /Ghi lượt khách/ })).not.toBeInTheDocument();
  });
});

describe("app shell", () => {
  it("folds the sidebar and remembers it on this device", async () => {
    serve(MANAGER);
    const user = userEvent.setup();
    at("/");
    await user.click(await screen.findByRole("button", { name: "Thu gọn thanh bên" }));
    expect(screen.getByRole("button", { name: "Mở rộng thanh bên" })).toHaveAttribute("aria-pressed", "true");
    expect(readSidebarCollapsed()).toBe(true);
  });

  it("opens the phone drawer with every screen and closes it after a choice", async () => {
    serve(MANAGER);
    const user = userEvent.setup();
    at("/");
    await user.click(await screen.findByRole("button", { name: "Thêm màn hình" }));
    const all = await screen.findByRole("navigation", { name: "Tất cả màn hình" });
    await user.click(within(all).getByRole("link", { name: "Dịch vụ" }));
    expect(screen.queryByRole("navigation", { name: "Tất cả màn hình" })).not.toBeInTheDocument();
  });

  it("keeps the quick lock one tap away", async () => {
    serve(MANAGER);
    at("/");
    expect(await screen.findByRole("button", { name: "Khóa nhanh" })).toBeInTheDocument();
  });
});

describe("settings", () => {
  it("switches the colour theme on this device", async () => {
    serve(MANAGER);
    const user = userEvent.setup();
    at("/cai-dat");
    await user.click(await screen.findByRole("button", { name: "Tối" }));
    expect(document.documentElement.dataset["theme"]).toBe("dark");
    expect(screen.getByRole("button", { name: "Tối" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText("0905 123 456")).toBeInTheDocument();
  });
});
