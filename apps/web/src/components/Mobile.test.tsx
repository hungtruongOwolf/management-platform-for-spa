import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FilterSelect, ResponsiveFilters } from "../features/reports/filters";
import { BottomSheet } from "./BottomSheet";
import { DateField, dmyToIso, isoToDmy } from "./DateField";
import { ResponsiveTable, type Column } from "./ResponsiveTable";
import { StickyActions } from "./StickyActions";

/** Pretend the viewport is a phone: every `max-width` query matches. */
function asPhone() {
  vi.stubGlobal(
    "matchMedia",
    (query: string) =>
      ({
        matches: query.includes("max-width"),
        media: query,
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
      }) as unknown as MediaQueryList,
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

interface Row {
  id: string;
  name: string;
  left: number;
  status: string;
  phone: string;
}
const rows: Row[] = [
  { id: "a", name: "Nguyễn Thị Hà", left: 3, status: "Đang dùng", phone: "0905 123 456" },
  { id: "b", name: "Trần Văn Nam", left: 0, status: "Hết buổi", phone: "0912 000 111" },
];
const columns: Column<Row>[] = [
  { id: "name", header: "Khách", card: "title", cell: (r) => <a href={`/khach-hang/${r.id}`}>{r.name}</a> },
  { id: "phone", header: "SĐT", card: "subtitle", cell: (r) => r.phone },
  { id: "left", header: "Còn lại", numeric: true, card: "aside", cell: (r) => `${String(r.left)} buổi` },
  { id: "status", header: "Trạng thái", cell: (r) => r.status },
];

describe("ResponsiveTable", () => {
  it("is a captioned table on wide screens (and without matchMedia)", () => {
    render(<ResponsiveTable caption="Gói liệu trình của khách" rows={rows} rowKey={(r) => r.id} columns={columns} />);
    const table = screen.getByRole("table", { name: "Gói liệu trình của khách" });
    expect(within(table).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Khách", "SĐT", "Còn lại", "Trạng thái"]);
    expect(within(table).getByRole("cell", { name: "3 buổi" })).toHaveClass("num");
  });

  it("is a list of cards on phones: title, subtitle, key fact and labelled fields, no table", () => {
    asPhone();
    render(<ResponsiveTable caption="Gói liệu trình của khách" rows={rows} rowKey={(r) => r.id} columns={columns} stretchLink />);
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    const list = screen.getByRole("list", { name: "Gói liệu trình của khách" });
    const [first] = within(list).getAllByRole("listitem");
    if (!first) throw new Error("no card");
    expect(within(first).getByRole("link", { name: "Nguyễn Thị Hà" })).toHaveAttribute("href", "/khach-hang/a");
    expect(first).toHaveTextContent("0905 123 456");
    expect(first).toHaveTextContent("3 buổi");
    // Fields keep their column header as the label.
    expect(within(first).getByText("Trạng thái").tagName).toBe("DT");
    expect(within(first).getByText("Đang dùng").tagName).toBe("DD");
    expect(list).toHaveClass("rt-stretch");
  });

  it("keeps fixed column widths, opens a row on click (not on its controls) and shows its details below", async () => {
    const user = userEvent.setup();
    const onRowClick = vi.fn();
    const sized = columns.map((c) => (c.id === "left" ? { ...c, width: "96px" } : c));
    const { rerender } = render(
      <ResponsiveTable caption="Gói" rows={rows} rowKey={(r) => r.id} columns={sized} onRowClick={onRowClick} />,
    );
    const table = screen.getByRole("table", { name: "Gói" });
    expect(table).toHaveClass("data-table-fixed");
    expect(table.querySelectorAll("col")[2]).toHaveStyle({ width: "96px" });
    await user.click(within(table).getByRole("cell", { name: "Hết buổi" }));
    expect(onRowClick).toHaveBeenCalledWith(rows[1]);
    await user.click(within(table).getByRole("link", { name: "Nguyễn Thị Hà" }));
    expect(onRowClick).toHaveBeenCalledOnce();
    rerender(
      <ResponsiveTable
        caption="Gói"
        rows={rows}
        rowKey={(r) => r.id}
        columns={sized}
        expanded={(r) => (r.id === "a" ? <p>Chi tiết của {r.name}</p> : null)}
      />,
    );
    const [, first, details] = within(table).getAllByRole("row");
    expect(first).toHaveClass("rt-open");
    expect(within(details as HTMLElement).getByRole("cell")).toHaveAttribute("colspan", "4");
    expect(details).toHaveTextContent("Chi tiết của Nguyễn Thị Hà");
  });

  it("on phones the whole card opens the row and the details go to the card's foot", async () => {
    asPhone();
    const user = userEvent.setup();
    const onRowClick = vi.fn();
    render(
      <ResponsiveTable
        caption="Gói"
        rows={rows}
        rowKey={(r) => r.id}
        columns={columns}
        onRowClick={onRowClick}
        expanded={(r) => (r.id === "b" ? <p>Chi tiết của {r.name}</p> : null)}
      />,
    );
    const [, second] = within(screen.getByRole("list", { name: "Gói" })).getAllByRole("listitem");
    await user.click(within(second as HTMLElement).getByText("Hết buổi"));
    expect(onRowClick).toHaveBeenCalledWith(rows[1]);
    expect(second).toHaveTextContent("Chi tiết của Trần Văn Nam");
  });
});

describe("BottomSheet", () => {
  it("is a labelled dialog with a close button, footer actions, and renders nothing while closed", async () => {
    const onClose = vi.fn();
    const { rerender } = render(
      <BottomSheet open={false} onClose={onClose} title="Ghi lần liên hệ" footer={<button type="button">Lưu</button>}>
        <p>Nội dung</p>
      </BottomSheet>,
    );
    expect(screen.queryByText("Nội dung")).not.toBeInTheDocument();
    rerender(
      <BottomSheet open onClose={onClose} title="Ghi lần liên hệ" description="Nguyễn Thị Hà" footer={<button type="button">Lưu</button>}>
        <p>Nội dung</p>
      </BottomSheet>,
    );
    const dialog = screen.getByRole("dialog", { name: "Ghi lần liên hệ" });
    expect(dialog).toHaveTextContent("Nguyễn Thị Hà");
    expect(within(dialog).getByRole("button", { name: "Lưu" })).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Đóng" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes on a backdrop press, not on a text selection dragged out of a field", () => {
    const onClose = vi.fn();
    render(
      <BottomSheet open onClose={onClose} title="Sửa hồ sơ">
        <input aria-label="Họ tên" defaultValue="Nguyễn Thị Hà" />
      </BottomSheet>,
    );
    const dialog = screen.getByRole("dialog", { name: "Sửa hồ sơ" });
    // Press in the field, release over the backdrop: the browser fires click on the dialog.
    fireEvent.pointerDown(screen.getByLabelText("Họ tên"));
    fireEvent.click(dialog);
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.pointerDown(dialog);
    fireEvent.click(dialog);
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe("StickyActions", () => {
  it("groups the actions under a name, with an optional summary", () => {
    render(
      <StickyActions label="Thanh toán" summary="Tổng 1.200.000đ">
        <button type="button">Lưu lượt</button>
      </StickyActions>,
    );
    const group = screen.getByRole("group", { name: "Thanh toán" });
    expect(group).toHaveTextContent("Tổng 1.200.000đ");
    expect(within(group).getByRole("button", { name: "Lưu lượt" })).toBeInTheDocument();
  });
});

describe("DateField", () => {
  it("parses dd/mm/yyyy (and bare digits), rejects impossible days", () => {
    expect(dmyToIso("05/11/2026")).toBe("2026-11-05");
    expect(dmyToIso("5/1/2026")).toBe("2026-01-05");
    expect(dmyToIso("05112026")).toBe("2026-11-05");
    expect(dmyToIso("31/02/2026")).toBeNull();
    expect(dmyToIso("2026-11-05")).toBeNull();
    expect(isoToDmy("2026-11-05")).toBe("05/11/2026");
  });

  it("shows the value as dd/mm/yyyy whatever the browser locale, commits ISO, flags out-of-range input", async () => {
    const onChange = vi.fn();
    render(<DateField label="Từ ngày" value="2026-11-05" max="2026-11-05" onChange={onChange} />);
    const box = screen.getByRole("textbox", { name: "Từ ngày" });
    expect(box).toHaveValue("05/11/2026");
    await userEvent.clear(box);
    await userEvent.type(box, "01112026{Enter}");
    expect(box).toHaveValue("01/11/2026");
    expect(onChange).toHaveBeenLastCalledWith("2026-11-01");
    await userEvent.clear(box);
    await userEvent.type(box, "06/11/2026{Enter}");
    expect(box).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent("dd/mm/yyyy");
    expect(screen.getByRole("button", { name: "Chọn từ ngày trên lịch" })).toBeInTheDocument();
  });
});

describe("ResponsiveFilters", () => {
  const filters = (onReset = vi.fn()) => (
    <ResponsiveFilters label="Lọc gói liệu trình" activeCount={1} onReset={onReset} search={<input aria-label="Tìm" />}>
      <FilterSelect label="Trạng thái" value="active" options={[["", "Mọi trạng thái"], ["active", "Đang dùng"]]} onChange={() => undefined} />
    </ResponsiveFilters>
  );

  it("shows every filter inline on wide screens", () => {
    render(filters());
    const bar = screen.getByRole("search", { name: "Lọc gói liệu trình" });
    expect(within(bar).getByRole("combobox", { name: "Trạng thái" })).toBeInTheDocument();
    expect(within(bar).queryByRole("button", { name: /Bộ lọc/ })).not.toBeInTheDocument();
  });

  it("on phones keeps search inline and moves the rest into a sheet behind a counted button", async () => {
    asPhone();
    const onReset = vi.fn();
    render(filters(onReset));
    const bar = screen.getByRole("search", { name: "Lọc gói liệu trình" });
    expect(within(bar).getByRole("textbox", { name: "Tìm" })).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Trạng thái" })).not.toBeInTheDocument();
    await userEvent.click(within(bar).getByRole("button", { name: "Bộ lọc, 1 đang chọn" }));
    const sheet = screen.getByRole("dialog", { name: "Lọc gói liệu trình" });
    expect(within(sheet).getByRole("combobox", { name: "Trạng thái" })).toBeInTheDocument();
    await userEvent.click(within(sheet).getByRole("button", { name: "Xóa bộ lọc" }));
    expect(onReset).toHaveBeenCalledTimes(1);
    await userEvent.click(within(sheet).getByRole("button", { name: "Xem kết quả" }));
    expect(screen.queryByRole("combobox", { name: "Trạng thái" })).not.toBeInTheDocument();
  });
});
