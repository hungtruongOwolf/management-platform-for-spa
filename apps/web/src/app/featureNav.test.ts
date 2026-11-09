import { describe, expect, it } from "vitest";
import type { Me } from "../features/auth/session";
import { hasFeature } from "../features/auth/session";
import { mayOpen } from "./access";
import { navFor } from "./nav";

const owner = (features?: string[]): Me => ({
  user: { id: "u1", full_name: "Chủ spa", phone: "0905 123 456" },
  active_tenant: { id: "t1", name: "Spa" },
  tenants: [{ id: "t1", name: "Spa" }],
  permissions: ["visit.record", "customer.read", "report.read", "appointment.manage", "catalog.read"],
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

  it("an API without the list (older) means every feature is on", () => {
    expect(hasFeature(owner(), "reports")).toBe(true);
    expect(hasFeature(null, "reports")).toBe(false);
  });
});
