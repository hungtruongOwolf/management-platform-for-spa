import type { ActivityEntry, ActivityPage } from "@vpyd/api-client";
import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { navFor } from "../../app/nav";
import { spaToday } from "../../lib/format";
import { ME } from "../reports/testServer";
import { activityParams } from "./api";
import { activitySearch } from "./search";
import { changeText, moneyOf, phraseOf, sentence } from "./sentence";

const OWNER = ["report.read", "customer.read", "visit.read", "audit.read", "staff.manage"];
/** The spa's features with "Nhật ký thao tác" switched on (off by default, ADR-0022). */
const SPA = (permissions: string[]) => ({
  ...ME(permissions),
  features: ["appointments", "payroll", "evaluations", "follow_ups", "notes_journal", "reports", "activity_log"],
});
const MANAGER = ["report.read", "customer.read", "visit.read"];
const HA = "0193a5b0-0000-7000-8000-0000000000d1";
const CUSTOMER = "0193a5b0-0000-7000-8000-0000000000c1";
const LAN = "0193a5b0-0000-7000-8000-0000000000a1";
const today = spaToday();

const by = (name: string) => ({ kind: "user" as const, user_id: HA, name });
const entry = (e: Partial<ActivityEntry> & Pick<ActivityEntry, "action" | "kind">): ActivityEntry => ({
  id: crypto.randomUUID(),
  at: `${today}T08:10:00Z`,
  day: today,
  actor: by("Quản lý Hà"),
  changes: [],
  packages: [],
  folded: [],
  ...e,
});

const MAI = { kind: "customer", id: CUSTOMER, name: "Lê Thị Mai", gender: "female", on: today } as const;
/** A void as the server folds it: the visit's line, its sale's reversal inside. */
const VOID = entry({
  action: "visit.void",
  kind: "front_desk",
  subject: MAI,
  amount: 140_000,
  reason: "khách đổi ý",
  folded: ["sale.void"],
});
/** A checkout that sold a package: one line, all the money collected. */
const CHECKOUT = entry({
  action: "visit.create",
  kind: "front_desk",
  subject: MAI,
  amount: 1_640_000,
  discount: 10_000,
  reason: "Khách quen",
  packages: ["Gội đầu 10 buổi"],
  folded: ["customer_package.create", "sale.create", "sale.create"],
});
const RENAMED = entry({
  action: "customer.update",
  kind: "customers",
  subject: { kind: "customer", id: CUSTOMER, name: "Lê Thị Mai", gender: "female" },
  changes: [
    { field: "full_name", format: "text", old: "Lê Mai", new: "Lê Thị Mai" },
    { field: "phone", format: "phone", old: "+84905111222", new: "+84912333444" },
    { field: "address", format: "text" },
  ],
});
const ATTENDANCE = entry({
  action: "attendance.record",
  kind: "staff",
  actor: by("Chủ Spa"),
  subject: { kind: "staff", id: LAN, name: "KTV Lan", on: "2026-11-01" },
  changes: [{ field: "status", format: "code", old: "present", new: "leave_dx" }],
  at: "2026-11-02T11:00:00Z",
  day: "2026-11-02",
});
const PRICE = entry({
  action: "catalog.service_update",
  kind: "settings",
  actor: by("Chủ Spa"),
  subject: { kind: "service", name: "Gội đầu dưỡng sinh" },
  changes: [{ field: "price", format: "money", old: "150000", new: "170000" }],
  at: "2026-11-02T03:00:00Z",
  day: "2026-11-02",
});
const LOCKED = entry({
  action: "account.locked_by_platform",
  kind: "accounts",
  actor: { kind: "platform" },
  subject: { kind: "account", name: "Quản lý Hà" },
  reason: "Chủ spa báo mất điện thoại",
  at: "2026-11-02T02:00:00Z",
  day: "2026-11-02",
});

function serve(permissions: string[], page: Partial<ActivityPage> = {}) {
  return vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const url = new URL(raw, "http://app.test");
    const p = url.pathname.replace(/^\/api\/v1/, "");
    const json = (body: unknown, status = 200) => Promise.resolve(Response.json(body, { status }));
    if (p === "/me") return json(SPA(permissions));
    if (p === "/audit-log") {
      if (!permissions.includes("audit.read")) return json({ status: 403, code: "auth.forbidden" }, 403);
      return json({
        from: "2026-10-27", to: today, items: [CHECKOUT, VOID, RENAMED, ATTENDANCE, PRICE, LOCKED], next_cursor: null,
        people: [
          { user_id: HA, name: "Quản lý Hà", left: false },
          { user_id: LAN, name: "KTV Lan", left: true },
        ],
        ...page,
      } satisfies ActivityPage);
    }
    return json({ status: 404, code: "not_found" }, 404);
  });
}

const at = (path: string) => {
  const history = createMemoryHistory({ initialEntries: [path] });
  render(<App history={history} />);
  return { search: () => Object.fromEntries(new URLSearchParams(history.location.search)) };
};

const calls = (spy: ReturnType<typeof serve>) =>
  spy.mock.calls
    .map(([input]) => new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url, "http://app.test"))
    .filter((u) => u.pathname.endsWith("/audit-log"));

describe("activity sentences", () => {
  it("words each group as one plain sentence", () => {
    expect(sentence(VOID)).toBe("Quản lý Hà hủy lượt của chị Lê Thị Mai");
    // What a cancel gave back (ADR-0022): "Trả lại", not the bookkeeping word "Đảo".
    expect(moneyOf(VOID)).toMatch(/^Trả lại 140\.000/);
    expect(sentence(RENAMED)).toBe("Quản lý Hà sửa hồ sơ của chị Lê Thị Mai");
    expect(RENAMED.changes.map(changeText)).toEqual(["Tên: Lê Mai → Lê Thị Mai", "SĐT: 0905 111 222 → 0912 333 444", "Địa chỉ (đã đổi)"]);
    expect(sentence(ATTENDANCE)).toBe("Chủ Spa chấm công cho KTV Lan ngày 01/11/2026");
    expect(ATTENDANCE.changes.map(changeText)).toEqual(["Trạng thái: Đi làm → Nghỉ đột xuất (ĐX)"]);
    expect(sentence(PRICE)).toBe("Chủ Spa sửa dịch vụ “Gội đầu dưỡng sinh”");
    expect(PRICE.changes.map(changeText)[0]).toMatch(/^Giá: 150\.000.*→ 170\.000/);
    expect(sentence(LOCKED)).toBe("Đội vận hành khóa tài khoản Quản lý Hà");
    expect(sentence(entry({ action: "account.password_reset_requested", kind: "accounts", actor: { kind: "system" }, subject: { kind: "account", name: "KTV Bình" } }))).toBe(
      "Hệ thống nhận yêu cầu quên mật khẩu của tài khoản KTV Bình",
    );
    expect(sentence(entry({ action: "payroll.close", kind: "payroll", detail: "2026-10", amount: 42_000_000 }))).toBe("Quản lý Hà chốt lương tháng 10/2026");
    expect(sentence(entry({ action: "staff.adjustment", kind: "staff", detail: "penalty", subject: { kind: "staff", name: "KTV Lan" } }))).toBe("Quản lý Hà ghi phạt cho KTV Lan");
    // A deleted group, named as its delete recorded it (spec record-history K68), like its other lines.
    const group = { kind: "service_category", name: "Nhóm thêm nhầm" } as const;
    expect(sentence(entry({ action: "catalog.category_delete", kind: "settings", actor: by("Chủ Spa"), subject: group }))).toBe(
      "Chủ Spa xóa nhóm dịch vụ “Nhóm thêm nhầm”",
    );
    expect(sentence(entry({ action: "catalog.category_create", kind: "settings", actor: by("Chủ Spa"), subject: group }))).toBe(
      "Chủ Spa thêm nhóm dịch vụ “Nhóm thêm nhầm”",
    );
  });

  it("reads one action as one line: a visit carries its sales, a package sale names its packages", () => {
    expect(sentence(CHECKOUT)).toBe("Quản lý Hà ghi lượt cho chị Lê Thị Mai kèm bán gói Gội đầu 10 buổi");
    expect(moneyOf(CHECKOUT)).toMatch(/^Thu 1\.640\.000/);
    expect(sentence(entry({ ...CHECKOUT, packages: [], folded: ["sale.create"] }))).toBe("Quản lý Hà ghi lượt cho chị Lê Thị Mai");
    const sale = entry({ action: "sale.create", kind: "front_desk", subject: MAI, amount: 1_500_000, packages: ["Gội đầu 10 buổi", "Xông hơi 5 buổi"], folded: ["customer_package.create", "customer_package.create"] });
    expect(sentence(sale)).toBe("Quản lý Hà bán gói Gội đầu 10 buổi, Xông hơi 5 buổi cho chị Lê Thị Mai");
    expect(sentence(entry({ action: "sale.void", kind: "front_desk", subject: MAI, packages: ["Gội đầu 10 buổi"] }))).toBe(
      "Quản lý Hà hủy hóa đơn bán gói Gội đầu 10 buổi của chị Lê Thị Mai",
    );
    // A sale no visit action wrote (a legacy row) still reads on its own.
    expect(sentence(entry({ action: "sale.create", kind: "front_desk", subject: MAI }))).toBe("Quản lý Hà thu tiền của chị Lê Thị Mai");
    expect(sentence(entry({ action: "visit.convert_to_package", kind: "front_desk", subject: MAI, packages: ["Gội đầu 10 buổi"] }))).toBe(
      "Quản lý Hà chuyển lượt sang gói Gội đầu 10 buổi cho chị Lê Thị Mai",
    );
  });

  it("never crashes on a legacy action or a gone subject", () => {
    const legacy = entry({ action: "visit.something_old", kind: "other", subject: { kind: "customer" } });
    expect(phraseOf(legacy).lead).toBe("làm một thao tác khác với");
    expect(sentence(legacy)).toBe("Quản lý Hà làm một thao tác khác với");
    const gone = entry({ action: "visit.void", kind: "front_desk", subject: { kind: "customer" } });
    expect(sentence(gone)).toBe("Quản lý Hà hủy lượt của một khách (đã xóa thông tin)");
    const walkIn = entry({ action: "visit.create", kind: "front_desk", subject: { kind: "customer", id: CUSTOMER, name: "Khách vãng lai" } });
    expect(sentence(walkIn)).toBe("Quản lý Hà ghi lượt cho khách vãng lai");
  });

  it("keeps only valid filters in the URL, ids and codes only", () => {
    expect(activitySearch.validate({ kind: "nope", actor: "0905123456", from: "2026-13-40" })).toEqual({});
    expect(activitySearch.validate({ actor: HA.toUpperCase(), kind: "staff" })).toEqual({ actor: HA, kind: "staff" });
    expect(activitySearch.validate({ actor: "platform" })).toEqual({ actor: "platform" });
    const params = activityParams(activitySearch.parse({ actor: "system", kind: "accounts", from: "2026-11-01", to: "2026-11-07" }));
    expect(Object.fromEntries(params)).toEqual({ actor: "system", kind: "accounts", from: "2026-11-01", to: "2026-11-07", limit: "50" });
  });

  it("puts 'Nhật ký thao tác' under Báo cáo for the owner only", () => {
    const report = (perms: string[]) => navFor(SPA(perms)).find((g) => g.label === "Báo cáo")?.items.map((i) => i.label);
    expect(report(OWNER)).toEqual(["Báo cáo kinh doanh", "Nhật ký ghi chú", "Nhật ký thao tác"]);
    expect(report(MANAGER)).toEqual(["Báo cáo kinh doanh", "Nhật ký ghi chú"]);
    // Switched off (the default): hidden from the owner too.
    const off = navFor(ME(OWNER)).find((g) => g.label === "Báo cáo")?.items.map((i) => i.label);
    expect(off).not.toContain("Nhật ký thao tác");
  });
});

describe("/nhat-ky-thao-tac", () => {
  it("lists entries by day: sentence, time, group, money, reason, changes, link to the customer", async () => {
    serve(OWNER);
    at("/nhat-ky-thao-tac");
    expect(await screen.findByRole("heading", { level: 1, name: "Nhật ký thao tác" })).toBeInTheDocument();
    const day = await screen.findByRole("region", { name: new RegExp(today.split("-").reverse().join("/")) });
    const rows = within(day).getAllByRole("listitem").filter((li) => li.classList.contains("activity-row"));
    expect(rows).toHaveLength(3);
    const [checkout, voided, renamed] = rows;
    if (!checkout || !voided || !renamed) throw new Error("rows");
    const said = (row: HTMLElement) => row.querySelector(".activity-sentence")?.textContent;
    // One line for the checkout (its sales folded in by the server), with all the money taken.
    expect(said(checkout)).toBe("Quản lý Hà ghi lượt cho chị Lê Thị Mai kèm bán gói Gội đầu 10 buổi");
    expect(checkout.querySelector(".activity-meta")?.textContent).toMatch(/Thu 1\.640\.000.*giảm 10\.000.*lý do: Khách quen/);
    expect(said(voided)).toBe("Quản lý Hà hủy lượt của chị Lê Thị Mai");
    expect(within(voided).getByRole("link", { name: "chị Lê Thị Mai" })).toHaveAttribute("href", `/khach-hang/${CUSTOMER}`);
    expect(within(voided).getByText("Quầy")).toBeInTheDocument();
    expect(within(voided).getByText("· lý do: khách đổi ý")).toBeInTheDocument();
    expect(within(renamed).getByText("SĐT: 0905 111 222 → 0912 333 444")).toBeInTheDocument();
    // The platform team is never named.
    expect(screen.getAllByRole("listitem").map((li) => said(li)).filter(Boolean)).toContain("Đội vận hành khóa tài khoản Quản lý Hà");
    // The person filter: members (a leaver marked), then the platform team and the system.
    const person = screen.getByRole("combobox", { name: "Người làm" });
    expect(within(person).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Mọi người", "Quản lý Hà", "KTV Lan (đã nghỉ)", "Đội vận hành", "Hệ thống",
    ]);
  });

  it("an empty value that is a setting reads as that setting, both ways (record-history K50)", async () => {
    const slot = (old: string | undefined, now: string | undefined, time: string) =>
      entry({
        action: "appointment.settings",
        kind: "settings",
        actor: by("Chủ Spa"),
        changes: [{ field: "max_per_slot", format: "number", ...(old ? { old } : {}), ...(now ? { new: now } : {}) }],
        at: `${today}T${time}:00Z`,
      });
    serve(OWNER, { items: [slot("6", undefined, "09:00"), slot(undefined, "6", "08:00")] });
    at("/nhat-ky-thao-tac");
    expect(await screen.findByText("Số khách tối đa mỗi khung giờ: 6 → không giới hạn")).toBeInTheDocument();
    expect(screen.getByText("Số khách tối đa mỗi khung giờ: không giới hạn → 6")).toBeInTheDocument();
    expect(screen.queryByText(/bỏ 6/)).toBeNull();
  });

  it("keeps filters in the URL and sends them to the API", async () => {
    const spy = serve(OWNER);
    const { search } = at("/nhat-ky-thao-tac");
    await screen.findAllByText(/Lê Thị Mai/);
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Loại việc" }), "staff");
    await waitFor(() => {
      expect(search()).toMatchObject({ kind: "staff" });
    });
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Người làm" }), HA);
    await waitFor(() => {
      expect(calls(spy).at(-1)?.searchParams.get("actor")).toBe(HA);
    });
    expect(calls(spy).at(-1)?.searchParams.get("kind")).toBe("staff");
    expect(search()).toMatchObject({ actor: HA, kind: "staff" });
  });

  it("does not open for a manager (the API would answer 403 anyway)", async () => {
    const spy = serve(MANAGER);
    at("/nhat-ky-thao-tac");
    expect(await screen.findByRole("heading", { name: "Không có quyền mở màn hình này" })).toBeInTheDocument();
    expect(calls(spy)).toHaveLength(0);
  });

  it("says so when nothing matches", async () => {
    serve(OWNER, { items: [] });
    at("/nhat-ky-thao-tac");
    expect(await screen.findByText("Chưa có thao tác nào")).toBeInTheDocument();
  });
});
