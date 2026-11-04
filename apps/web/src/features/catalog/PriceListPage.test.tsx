import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";

const ME = {
  user: { id: "u1", full_name: "Quản lý Ngọc", phone: "0905 123 456" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions: ["catalog.read"],
};
const CATEGORIES = [
  { id: "c1", name: "Gội đầu dưỡng sinh", sort_order: 0 },
  { id: "c2", name: "Dịch vụ bổ sung", sort_order: 1 },
];
const SERVICES = [
  { id: "s1", category_id: "c1", name: "Thư Giãn", description: "+ Cạo gió đầu, đá nóng", duration_min: 45, recommended: false, active: true, prices: { female: 79_000, male: 99_000 } },
  { id: "s2", category_id: "c1", name: "Chuyên Sâu Đặc Biệt", duration_min: 60, recommended: true, active: true, prices: { female: 149_000, male: 199_000 } },
  { id: "s3", category_id: "c2", name: "Mặt nạ thuốc bắc", duration_min: null, recommended: false, active: true, prices: { all: 50_000 } },
];

function serve(responses: Record<string, () => Response>) {
  vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.pathname : input.url;
    return Promise.resolve(responses[path]?.() ?? new Response(null, { status: 404 }));
  });
}

describe("PriceListPage", () => {
  it("lists services by category with gender prices, flat prices and consult durations", async () => {
    serve({
      "/api/v1/me": () => Response.json(ME),
      "/api/v1/service-categories": () => Response.json(CATEGORIES),
      "/api/v1/services": () => Response.json(SERVICES),
    });
    render(<App history={createMemoryHistory({ initialEntries: ["/bang-gia"] })} />);

    const shampoo = await screen.findByRole("region", { name: "Gội đầu dưỡng sinh" });
    const thuGian = within(shampoo).getByText("Thư Giãn").closest("li");
    expect(thuGian).toHaveTextContent("45 phút · + Cạo gió đầu, đá nóng");
    expect(thuGian).toHaveTextContent("Nữ79.000đ");
    expect(thuGian).toHaveTextContent("Nam99.000đ");
    expect(within(shampoo).getByLabelText("Khuyên dùng")).toBeInTheDocument();

    const extras = screen.getByRole("region", { name: "Dịch vụ bổ sung" });
    const mask = within(extras).getByText("Mặt nạ thuốc bắc").closest("li");
    expect(mask).toHaveTextContent("Theo tư vấn");
    expect(mask).toHaveTextContent("50.000đ");
    expect(mask).not.toHaveTextContent("Nữ");
  });

  it("shows a Vietnamese error when the API refuses", async () => {
    serve({
      "/api/v1/me": () => Response.json(ME),
      "/api/v1/service-categories": () => new Response(JSON.stringify({ code: "auth.forbidden" }), { status: 403 }),
      "/api/v1/services": () => new Response(JSON.stringify({ code: "auth.forbidden" }), { status: 403 }),
    });
    render(<App history={createMemoryHistory({ initialEntries: ["/bang-gia"] })} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Bạn không có quyền thực hiện thao tác này.");
  });

  it("hides the menu link from users without catalog.read (UX only)", async () => {
    serve({ "/api/v1/me": () => Response.json({ ...ME, permissions: [] }) });
    render(<App history={createMemoryHistory({ initialEntries: ["/"] })} />);
    expect(await screen.findByRole("link", { name: "Trang chủ" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Bảng giá" })).not.toBeInTheDocument();
  });
});
