import { describe, expect, it } from "vitest";
import type { Me } from "../features/auth/session";
import { hasFeature } from "../features/auth/session";
import { mayOpen } from "./access";
import { navFor } from "./nav";

const owner = (features?: string[]): Me => ({
  user: { id: "u1", full_name: "Chủ spa", phone: "0905 123 456" },
  active_tenant: { id: "t1", name: "Spa" },
  tenants: [{ id: "t1", name: "Spa" }],
  permissions: ["visit.record", "visit.read", "customer.read", "report.read", "appointment.manage", "catalog.read", "staff.read_own"],
  ...(features ? { features } : {}),
});

const labels = (me: Me) => navFor(me).flatMap((g) => g.items.map((i) => i.label));

describe("feature switches in the spa's UI (ADR-0018)", () => {
  it("hides the reports and the notes journal when the spa has them off", () => {
    const all = owner(["appointments", "payroll", "evaluations", "follow_ups", "notes_journal", "reports"]);
    expect(labels(all)).toEqual(expect.arrayContaining(["Báo cáo kinh doanh", "Nhật ký ghi chú"]));
    const off = owner(["appointments", "payroll", "evaluations", "follow_ups"]);
    expect(labels(off)).not.toContain("Báo cáo kinh doanh");
    expect(labels(off)).not.toContain("Nhật ký ghi chú");
    expect(mayOpen.reports(off)).toBe(false);
    expect(mayOpen.notes(off)).toBe(false);
    // The rest of the management views stay.
    expect(labels(off)).toContain("Gói liệu trình của khách");
  });

  it("'Khách của tôi' needs the appointments feature: its only API is GET /me/appointments", () => {
    const tech = (features?: string[]): Me => ({ ...owner(features), permissions: ["catalog.read", "staff.read_basic", "staff.read_own"] });
    expect(mayOpen.myWork(tech(["appointments"]))).toBe(true);
    expect(labels(tech(["appointments"]))).toContain("Khách của tôi");
    expect(mayOpen.myWork(tech(["payroll"]))).toBe(false);
    expect(labels(tech(["payroll"]))).not.toContain("Khách của tôi");
  });

  it("the package overview and the absent list also need what their API checks (roles are editable)", () => {
    const role = (permissions: string[]): Me => ({ ...owner(), permissions });
    expect(mayOpen.packages(role(["report.read"]))).toBe(false);
    expect(mayOpen.packages(role(["report.read", "customer.read"]))).toBe(true);
    expect(mayOpen.inactive(role(["report.read", "customer.read"]))).toBe(false);
    expect(mayOpen.inactive(role(["report.read", "customer.read", "visit.read"]))).toBe(true);
    expect(labels(role(["report.read", "customer.read"]))).not.toContain("Khách lâu chưa quay lại");
  });

  it("an API without the list (older) means each feature's default", () => {
    expect(hasFeature(owner(), "reports")).toBe(true);
    expect(hasFeature(null, "reports")).toBe(false);
    expect(hasFeature(owner(), "activity_log", { defaultOff: true })).toBe(false);
    expect(hasFeature(owner(["activity_log"]), "activity_log", { defaultOff: true })).toBe(true);
    expect(hasFeature(null, "activity_log", { defaultOff: true })).toBe(false);
  });

  it("'Nhật ký thao tác' needs audit.read and the activity_log switch, off by default (ADR-0022)", () => {
    const withAudit = (features?: string[]): Me => ({ ...owner(features), permissions: [...owner().permissions, "audit.read"] });
    const defaults = ["appointments", "payroll", "evaluations", "follow_ups", "notes_journal", "reports"];
    expect(mayOpen.activity(withAudit(defaults))).toBe(false);
    expect(labels(withAudit(defaults))).not.toContain("Nhật ký thao tác");
    expect(mayOpen.activity(withAudit())).toBe(false);
    const on = withAudit([...defaults, "activity_log"]);
    expect(mayOpen.activity(on)).toBe(true);
    expect(labels(on)).toContain("Nhật ký thao tác");
    // The switch never opens it without the right.
    expect(mayOpen.activity(owner([...defaults, "activity_log"]))).toBe(false);
  });
});
