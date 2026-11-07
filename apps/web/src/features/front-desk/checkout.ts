// Round 6 checkout rules that only matter once a package is sold in the cart (spec R1-1 §10.4.1):
// the wire form of the packages and the package-only sale. Pure and React-free; lives in the lazy
// cart chunk (`CartPackages`), off the startup bundle.
import type {
  PaymentCreate,
  Sale,
  SaleCheckout,
  VisitPackageSaleCreate,
} from "@vpyd/api-client";
import { api } from "../../lib/api";
import { createSale } from "../sales/api";
import { boughtForOtherText } from "../sales/saleText";
import { type PackageSaleDraft, packageDiscount } from "./api";

/** "Khách tự chọn" in the closer picker: nobody is credited (Q-U2). */
export const SELF_CHOSEN = "self";

/** The wire form of the packages sold in a checkout (plans not chosen yet are left out). */
export function packageSalesBody(
  sales: PackageSaleDraft[],
): VisitPackageSaleCreate[] {
  return sales.flatMap((p) => {
    if (!p.plan) return [];
    const d = packageDiscount(p);
    return [
      {
        ref: p.ref,
        package_plan_id: p.plan.id,
        closer_staff_id:
          p.closer === SELF_CHOSEN || p.closer === "" ? null : p.closer,
        gift_note: p.giftNote.trim(),
        discount: d > 0 ? { amount: d, reason: p.discountReason.trim() } : null,
        beneficiary_customer_id: p.beneficiary?.id ?? null,
      },
    ];
  });
}

/**
 * A cart with one package and no service is a plain package sale (`POST /sales`, US10): no visit,
 * the technician's customer count does not move.
 */
export async function sellPackageOnly(
  customerId: string,
  sale: PackageSaleDraft,
  payments: PaymentCreate[],
  idempotencyKey: string,
): Promise<Sale> {
  const [body] = packageSalesBody([sale]);
  if (!body) throw new Error("no plan chosen");
  return createSale(
    {
      customer_id: customerId,
      closer_staff_id: body.closer_staff_id ?? null,
      beneficiary_customer_id: body.beneficiary_customer_id ?? null,
      lines: [
        {
          package_plan_id: body.package_plan_id,
          gift_note: body.gift_note ?? "",
        },
      ],
      discount: body.discount ?? null,
      payments,
    },
    idempotencyKey,
  );
}

/**
 * A cart of several packages and no service (EM 2026-11-07 item 4, "chị Lan"): one save, one
 * payment step (`POST /sales/checkout`). Each package is its own sale with its own plan,
 * beneficiary, discount and closer; the server splits the payment over them and saves all or none.
 */
export async function sellPackagesOnly(
  customerId: string,
  sales: PackageSaleDraft[],
  payments: PaymentCreate[],
  idempotencyKey: string,
): Promise<SaleCheckout> {
  const packages = packageSalesBody(sales).map((p) => ({
    package_plan_id: p.package_plan_id,
    closer_staff_id: p.closer_staff_id ?? null,
    gift_note: p.gift_note ?? "",
    discount: p.discount ?? null,
    beneficiary_customer_id: p.beneficiary_customer_id ?? null,
  }));
  return (await api("/sales/checkout", {
    method: "POST",
    body: { customer_id: customerId, package_sales: packages, payments },
    headers: { "Idempotency-Key": idempotencyKey },
  })) as SaleCheckout;
}

/**
 * Saves a cart of packages without services and says what was sold: one package → `POST /sales`
 * (as before), several → `POST /sales/checkout` (one save, one payment). Returns the toast text,
 * one sentence per package (a package bought for someone else names both people).
 */
export async function sellCart(
  payer: { id: string; full_name: string },
  drafts: PackageSaleDraft[],
  payments: PaymentCreate[],
  idempotencyKey: string,
): Promise<string> {
  const [only] = drafts;
  const sold =
    drafts.length === 1 && only
      ? [await sellPackageOnly(payer.id, only, payments, idempotencyKey)]
      : (await sellPackagesOnly(payer.id, drafts, payments, idempotencyKey)).sales;
  return drafts
    .map((d, i) => {
      const name = (sold[i]?.lines ?? []).map((l) => l.description).join(", ");
      return d.beneficiary
        ? boughtForOtherText(name, d.beneficiary.full_name, payer.full_name)
        : `Đã bán ${name} · ${payer.full_name}`;
    })
    .join(" ");
}
