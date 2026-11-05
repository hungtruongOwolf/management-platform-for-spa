import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { lastHrefFor, previousEntry, recordLocation, resetTrail, setTitle } from "./trail";

beforeEach(() => {
  resetTrail();
});
afterEach(() => {
  sessionStorage.clear();
});

describe("navigation trail", () => {
  it("names the previous screen, with its list state, for the back control", () => {
    recordLocation(0, "/goi-lieu-trinh?status=cancelled", "/goi-lieu-trinh");
    setTitle(0, "/goi-lieu-trinh?status=cancelled", "/goi-lieu-trinh", "Gói liệu trình của khách");
    recordLocation(1, "/khach-hang/c1", "/khach-hang/c1");
    expect(previousEntry(1, "/khach-hang/c1")).toEqual({
      href: "/goi-lieu-trinh?status=cancelled",
      pathname: "/goi-lieu-trinh",
      title: "Gói liệu trình của khách",
    });
  });

  it("keeps a title set before the router reports the navigation", () => {
    setTitle(3, "/bao-cao", "/bao-cao", "Tổng quan hôm nay");
    recordLocation(3, "/bao-cao?date=2026-11-01", "/bao-cao");
    recordLocation(4, "/khach-hang/c1", "/khach-hang/c1");
    expect(previousEntry(4, "/khach-hang/c1")?.title).toBe("Tổng quan hôm nay");
    expect(previousEntry(4, "/khach-hang/c1")?.href).toBe("/bao-cao?date=2026-11-01");
  });

  it("a replace on the same screen updates the URL and keeps the title", () => {
    recordLocation(0, "/khach-hang", "/khach-hang");
    setTitle(0, "/khach-hang", "/khach-hang", "Khách hàng");
    recordLocation(0, "/khach-hang?q=ha", "/khach-hang");
    expect(lastHrefFor("/khach-hang", 1)).toBe("/khach-hang?q=ha");
    recordLocation(1, "/khach-hang/c1", "/khach-hang/c1");
    expect(previousEntry(1, "/khach-hang/c1")?.title).toBe("Khách hàng");
  });

  it("has no back target after a deep link, or towards the sign-in screens", () => {
    recordLocation(0, "/khach-hang/c1", "/khach-hang/c1");
    expect(previousEntry(0, "/khach-hang/c1")).toBeUndefined();
    recordLocation(4, "/dang-nhap", "/dang-nhap");
    recordLocation(5, "/", "/");
    expect(previousEntry(5, "/")).toBeUndefined();
  });

  it("ignores entries ahead of the current one (a discarded forward stack)", () => {
    recordLocation(0, "/khach-hang?q=a", "/khach-hang");
    recordLocation(5, "/khach-hang?q=zzz", "/khach-hang");
    expect(lastHrefFor("/khach-hang", 2)).toBe("/khach-hang?q=a");
  });

  it("survives a reload through sessionStorage, and storage failures", async () => {
    recordLocation(0, "/goi-lieu-trinh?status=active", "/goi-lieu-trinh");
    vi.resetModules();
    const fresh = await import("./trail");
    expect(fresh.lastHrefFor("/goi-lieu-trinh", 0)).toBe("/goi-lieu-trinh?status=active");

    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("quota");
    });
    expect(() => {
      fresh.recordLocation(1, "/", "/");
    }).not.toThrow();
  });
});
