// Round 6 checkout rules that only matter once a package is sold in the cart (spec R1-1 §10.4.1):
// the wire form of the packages and the package-only sale. Pure and React-free; lives in the lazy
// cart chunk (`CartPackages`), off the startup bundle.
import type {
  PaymentCreate,
  Sale,
  VisitPackageSaleCreate,
} from "@vpyd/api-client";
import { createSale } from "../sales/api";
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
