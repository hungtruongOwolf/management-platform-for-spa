// Round 6 cart rules (spec R1-1 §10.4.1, §10.8): which lines a package sold today covers, the
// wire form, the checks before sending, and the draft that survives a reload.
import { describe, expect, it } from "vitest";
import { ApiError } from "../../lib/api";
import {
  type CartLine,
  type PackageSaleDraft,
  amountDue,
  coverage,
  packagesDue,
  visitBody,
} from "./api";
import { checkoutProblem, saleErrorOf, saleProblem } from "./CartPackages";
import { SELF_CHOSEN, packageSalesBody } from "./checkout";
import {
  latestDraftCustomer,
  restoreCart,
  saveDraft,
  type CartDraft,
} from "./draftCart";

const oneOff = (
  key: number,
  serviceId: string,
  price: number,
): Extract<CartLine, { source: "one_off" }> => ({
  key,
  source: "one_off",
  serviceId,
  serviceName: `Dịch vụ ${serviceId}`,
  priceVariant: null,
  price,
  staffId: "st1",
});

const plan = (sessions: number, bonus = 0) => ({
  id: "pl1",
  name: "Gói Đại Thông Lạc",
  price: 3_500_000,
  items: [{ service_id: "s5", sessions, bonus_sessions: bonus }],
});

const sale = (over: Partial<PackageSaleDraft> = {}): PackageSaleDraft => ({
  ref: "p1",
  plan: plan(10, 2),
  closer: "st1",
  giftNote: "",
  discountText: "",
  discountReason: "",
  useToday: true,
  beneficiary: null,
  ...over,
});

describe("round 6 cart", () => {
  it("covers today's one-off lines of the package's services, up to its sessions (AC9.1)", () => {
    const lines = [
      oneOff(1, "s5", 399_000),
      oneOff(2, "s5", 399_000),
      oneOff(3, "s10", 100_000),
    ];
    const one = coverage(lines, [sale({ plan: plan(1) })]);
    expect([...one.entries()]).toEqual([[1, "p1"]]);
    const all = coverage(lines, [sale()]);
    expect([...all.keys()]).toEqual([1, 2]);
    // Covered lines cost nothing now; the package is due instead.
    expect(amountDue(lines, all)).toBe(100_000);
    expect(packagesDue([sale()])).toBe(3_500_000);
  });

  it("never covers with a package bought for someone else or left unticked (D5, Q-U1)", () => {
    const lines = [oneOff(1, "s5", 399_000)];
    expect(coverage(lines, [sale({ useToday: false })]).size).toBe(0);
    expect(coverage(lines, [sale({ forOther: true })]).size).toBe(0);
    expect(
      coverage(lines, [sale({ beneficiary: { id: "c2", full_name: "Con" } })])
        .size,
    ).toBe(0);
  });

  it("sends covered lines as today's session of the new package, with referrer and companions", () => {
    const lines: CartLine[] = [
      oneOff(1, "s5", 399_000),
      { ...oneOff(2, "s10", 100_000), referrerId: "st2" },
    ];
    const sales = [
      sale({ discountText: "100.000", discountReason: "Khách quen" }),
    ];
    const body = visitBody("c1", lines, [], "", {
      packageSales: packageSalesBody(sales),
      covered: coverage(lines, sales),
      companions: ["  Chồng của chị Hà ", " "],
    });
    expect(body.lines).toEqual([
      {
        service_id: "s5",
        source: "package",
        package_ref: "p1",
        staff_id: "st1",
      },
      {
        service_id: "s10",
        source: "one_off",
        staff_id: "st1",
        price_variant: null,
        referrer_staff_id: "st2",
      },
    ]);
    expect(body.package_sales).toEqual([
      {
        ref: "p1",
        package_plan_id: "pl1",
        closer_staff_id: "st1",
        gift_note: "",
        discount: { amount: 100_000, reason: "Khách quen" },
        beneficiary_customer_id: null,
      },
    ]);
    expect(body.companions).toEqual(["Chồng của chị Hà"]);
    // Nothing about prices goes to the server.
    expect(JSON.stringify(body)).not.toMatch(/price"|total|3500000/);
  });

  it("sends khách tự chọn as no closer and leaves out blocks without a plan", () => {
    const body = packageSalesBody([
      sale({ closer: SELF_CHOSEN }),
      sale({ ref: "p2", plan: null }),
    ]);
    expect(body).toHaveLength(1);
    expect(body[0]?.closer_staff_id).toBeNull();
  });

  it("checks each package before sending: plan, closer, beneficiary, discount cap", () => {
    expect(saleProblem(sale({ plan: null }), false)).toBe(
      "Chọn mẫu gói liệu trình.",
    );
    expect(saleProblem(sale({ closer: "" }), false)).toMatch(
      /người giới thiệu/,
    );
    expect(saleProblem(sale({ forOther: true }), false)).toBe(
      "Chọn khách sẽ dùng gói.",
    );
    expect(saleProblem(sale({ discountText: "abc" }), false)).toMatch(/chữ số/);
    expect(saleProblem(sale({ discountText: "100000" }), false)).toMatch(
      /lý do/,
    );
    // Manager: at most 10 % of the package (D10, Q-U9); the owner has no cap.
    const big = { discountText: "400000", discountReason: "VIP" };
    expect(saleProblem(sale(big), false)).toMatch(/10 %/);
    expect(saleProblem(sale(big), true)).toBeNull();
    expect(checkoutProblem([sale()], 0, true, false)).toMatch(
      /Lịch hẹn cần ít nhất một dịch vụ/,
    );
    expect(checkoutProblem([sale()], 1, true, false)).toBeNull();
  });

  it("shows a refusal at the package blocks only when it concerns them", () => {
    const pkgErr = new ApiError(422, "validation_failed", [
      { field: "package_sales[0].package_plan_id", code: "inactive" },
    ]);
    expect(saleErrorOf(pkgErr)).not.toBeNull();
    expect(saleErrorOf(new ApiError(409, "visit.voided"))).toBeNull();
    expect(saleErrorOf(new Error("offline"))).toBeNull();
  });

  it("keeps the cart draft per user and customer, and refuses a draft of another shape", () => {
    const draft: CartDraft = {
      v: 1,
      customer: {
        id: "c1",
        full_name: "Nguyễn Thị Hà",
        phone: "0905",
        gender: "female",
      },
      lines: [oneOff(1, "s5", 399_000)],
      sales: [sale()],
      mode: "bank_transfer",
      cashText: "",
      discount: null,
      serviceDay: "",
      serviceTime: "",
      companions: [],
      appointment: null,
    };
    saveDraft("u1", "c1", { ...draft, note: "Huyết áp cao" }, 1_000);
    saveDraft(
      "u1",
      "c2",
      { ...draft, customer: { ...draft.customer, id: "c2" } },
      2_000,
    );
    saveDraft("u2", "c3", draft, 3_000);
    expect(latestDraftCustomer("u1", 2_500)).toBe("c2");
    const raw = JSON.parse(
      sessionStorage.getItem("vpyd.cart.v1:u1:c1") ?? "{}",
    ) as {
      draft: object;
    };
    // The session note (health data) is never stored.
    expect(raw.draft).not.toHaveProperty("note");
    expect(restoreCart(raw.draft)).toMatchObject({
      mode: "bank_transfer",
      sales: [{ ref: "p1" }],
    });
    expect(restoreCart({ ...draft, v: 2 })).toBeNull();
    expect(restoreCart({ ...draft, lines: [{ key: "x" }] })).toBeNull();
  });
});
