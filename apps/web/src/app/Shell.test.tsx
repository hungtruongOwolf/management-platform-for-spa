import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Me } from "../features/auth/session";
import { serveManagement } from "../features/reports/testServer";
import { App } from "./App";
import { fold } from "./CommandPalette";
import { canSeeManagement } from "./features";
import { navFor, navKey, tabsFor } from "./nav";
import { applyTheme, readSidebarCollapsed, readTheme, saveSidebarCollapsed } from "./prefs";
import { ThemePicker } from "./ThemePicker";
import { initialOf, roleLabel } from "./UserMenu";

const me = (permissions: string[]): Me => ({
  user: { id: "u1", full_name: "Nguyễn Thị Thu Hà", phone: "0905 123 456" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions,
});
const MANAGER = ["visit.record", "customer.read", "catalog.read", "report.read"];

// The shell's home page asks the management API for today's numbers: use the contract-shaped fake.
const serve = (permissions: string[], searchHits: unknown[] = []) => serveManagement({ permissions, searchHits });

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

  it("puts Lịch hẹn in place of home on the tab bar for whoever books appointments (EM round 4)", () => {
    const tabs = tabsFor(navFor(me([...MANAGER, "appointment.manage"])));
    expect(tabs.map(navKey)).toEqual(["/quay?view=lich", "/quay", "/khach-hang", "/bao-cao"]);
    expect(tabs.map((t) => t.short)).toEqual(["Lịch hẹn", "Quầy", "Khách", "Báo cáo"]);
    // The record tab is only active without `view`, so the two never light up together.
    expect(tabs[1]?.search).toEqual({ view: undefined });
  });
});

describe("top bar (round 2: date and account as visible components)", () => {
  it("links today's date to the calendar for whoever books appointments", async () => {
    serve([...MANAGER, "appointment.manage"]);
    at("/bang-gia");
    const chip = await screen.findByRole("link", { name: /mở lịch hẹn hôm nay$/ });
    expect(chip).toHaveAttribute("href", "/quay?view=lich");
  });

  it("shows the date without a link to everyone else", async () => {
    serve(MANAGER);
    at("/bang-gia");
    await screen.findByRole("navigation", { name: "Chính" });
    expect(screen.queryByRole("link", { name: /mở lịch hẹn hôm nay$/ })).not.toBeInTheDocument();
  });

  it("takes the avatar initial from the last word with a letter", () => {
    expect(initialOf("Nguyễn Thị Thu Hà")).toBe("H");
    expect(initialOf("Chủ spa (test)")).toBe("T");
    expect(initialOf("  ")).toBe("");
  });
});

describe("management views gate", () => {
  it("opens with the API's report.read only", () => {
    expect(canSeeManagement(me(["report.read"]))).toBe(true);
    expect(canSeeManagement(me(["sale.read", "report.revenue.read"]))).toBe(false);
    expect(canSeeManagement(null)).toBe(false);
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

describe("theme pickers", () => {
  it("share one value: choosing in the account menu updates every picker on screen", async () => {
    const user = userEvent.setup();
    render(
      <>
        <ThemePicker />
        <ThemePicker />
      </>,
    );
    await user.click(screen.getAllByRole("button", { name: "Tối" })[0] as HTMLElement);
    for (const b of screen.getAllByRole("button", { name: "Tối" })) expect(b).toHaveAttribute("aria-pressed", "true");
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
    await user.click(await screen.findByRole("button", { name: /Tìm khách, mở màn hình/ }));
    const box = await screen.findByRole("combobox", { name: "Tìm khách hoặc màn hình" });
    await user.type(box, "mau goi");
    const screens = await screen.findByRole("group", { name: "Màn hình" });
    expect(within(screens).getAllByRole("option").map((o) => o.textContent)).toEqual([
      expect.stringContaining("Mẫu gói liệu trình"),
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
    await user.click(within(all).getByRole("link", { name: "Bảng giá dịch vụ" }));
    expect(screen.queryByRole("navigation", { name: "Tất cả màn hình" })).not.toBeInTheDocument();
  });

  it("account menu: who is signed in, theme, quick lock and sign out; Esc closes it", async () => {
    serve(MANAGER);
    const user = userEvent.setup();
    at("/");
    const button = await screen.findByRole("button", { name: "Tài khoản: Quản lý Hà, Quản lý" });
    expect(button).toHaveAttribute("aria-expanded", "false");
    await user.click(button);
    const panel = await screen.findByRole("group", { name: "Tài khoản" });
    expect(within(panel).getByText("Quản lý · Viên Phúc Y Đường")).toBeInTheDocument();
    expect(within(panel).getByRole("group", { name: "Chế độ màu" })).toBeInTheDocument();
    expect(within(panel).getByRole("button", { name: /^Khóa nhanh/ })).toBeInTheDocument();
    expect(within(panel).getByRole("button", { name: "Đăng xuất" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("group", { name: "Tài khoản" })).not.toBeInTheDocument();
    expect(button).toHaveFocus();
  });

  it("names the role from the permissions (display only)", () => {
    expect(roleLabel(me(["catalog.manage", "visit.record"]))).toBe("Chủ spa");
    expect(roleLabel(me(MANAGER))).toBe("Quản lý");
    expect(roleLabel(me(["catalog.read"]))).toBe("Kỹ thuật viên");
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
