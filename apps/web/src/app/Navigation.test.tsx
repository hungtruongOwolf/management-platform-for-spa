import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { serveManagement } from "../features/reports/testServer";
import { App } from "./App";
import { CustomerName } from "./CustomerName";
import { resetTrail } from "./trail";

const MANAGER = ["visit.record", "customer.read", "catalog.read", "report.read"];

function at(path: string) {
  const history = createMemoryHistory({ initialEntries: [path] });
  render(<App history={history} />);
  return history;
}

beforeEach(() => {
  resetTrail();
});

describe("navigation model (EM round 3: always a way back)", () => {
  it("list → profile → back returns to the list with its filter, and the sidebar marks the section", async () => {
    serveManagement({ permissions: MANAGER });
    const user = userEvent.setup();
    const history = at("/goi-lieu-trinh?status=cancelled");
    const table = await screen.findByRole("table", { name: "Gói liệu trình của khách" });
    expect(screen.getByRole("button", { name: /Đã hủy/ })).toHaveAttribute("aria-pressed", "true");

    await user.click(within(table).getAllByRole("link")[0] as HTMLElement);
    expect(await screen.findByRole("heading", { level: 1, name: /^Nguyễn Thị Hà/ })).toBeInTheDocument();
    const crumbs = screen.getByRole("navigation", { name: "Vị trí trang" });
    expect(within(crumbs).getByRole("link", { name: "Khách hàng" })).toHaveAttribute("href", "/khach-hang");
    // The profile belongs to the customers section, whichever list it was opened from.
    expect(within(screen.getByRole("navigation", { name: "Chính" })).getByRole("link", { name: "Hồ sơ khách" })).toHaveClass("active");

    const back = screen.getByRole("link", { name: "Quay lại Theo dõi gói của khách" });
    expect(back).toHaveAttribute("href", "/goi-lieu-trinh?status=cancelled");
    await user.click(back);
    expect(await screen.findByRole("heading", { level: 1, name: "Theo dõi gói của khách" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Đã hủy/ })).toHaveAttribute("aria-pressed", "true");
    expect(history.location.search).toContain("status=cancelled");
  });

  it("after a deep link, back goes to the logical parent", async () => {
    serveManagement({ permissions: MANAGER });
    at("/khach-hang/c1");
    expect(await screen.findByRole("heading", { level: 1, name: /^Nguyễn Thị Hà/ })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Quay lại Khách hàng" })).toHaveAttribute("href", "/khach-hang");
  });

  it("filters write to the URL (replace, not push) and the default drops out", async () => {
    serveManagement({ permissions: MANAGER });
    const user = userEvent.setup();
    const history = at("/khach-lau-khong-quay-lai");
    await screen.findByRole("table", { name: "Khách lâu không quay lại" });
    const entries = history.length;
    await user.click(screen.getByRole("button", { name: "≥ 30 ngày" }));
    await user.click(screen.getByRole("button", { name: "Mọi khách đã từng đến" }));
    expect(history.location.search).toContain("days=30");
    expect(history.location.search).toContain("scope=all");
    expect(history.length).toBe(entries);
    await user.click(screen.getByRole("button", { name: "≥ 7 ngày" }));
    expect(history.location.search).not.toContain("days=");
  });

  it("the customers search box starts from the URL's q", async () => {
    serveManagement({ permissions: MANAGER, searchHits: [] });
    at("/khach-hang?q=ha%20noi");
    expect(await screen.findByRole("combobox", { name: "Tìm khách theo SĐT hoặc tên" })).toHaveValue("ha noi");
  });
});

describe("CustomerName", () => {
  it("puts the gender right beside the name, as text", () => {
    render(
      <ul>
        <li>
          <CustomerName name="Nguyễn Thị Hà" gender="female" />
        </li>
        <li>
          <CustomerName name="Trần Văn Nam" gender="male" />
        </li>
        <li>
          <CustomerName name="Lê An" gender={null} />
        </li>
        <li>
          <CustomerName name="Lê Bình" showUnknown />
        </li>
      </ul>,
    );
    const items = screen.getAllByRole("listitem");
    expect(items[0]).toHaveTextContent("Nguyễn Thị Hà, Nữ");
    expect(items[1]).toHaveTextContent("Trần Văn Nam, Nam");
    expect(items[2]).toHaveTextContent(/^Lê An$/);
    expect(items[3]).toHaveTextContent("Lê Bình, giới tính Chưa rõ");
  });
});
