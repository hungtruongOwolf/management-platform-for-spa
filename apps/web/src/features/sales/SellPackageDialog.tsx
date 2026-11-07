import type { PaymentCreate, Sale } from "@vpyd/api-client";
import {
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import { useCallback, useId, useState } from "react";
import { StickyActions } from "../../components/StickyActions";
import { fieldMessage, messageFor } from "../../lib/errors";
import { invalidateManagementViews } from "../../lib/invalidate";
import { can, meQuery } from "../auth/session";
import { customerQuery } from "../customers/api";
// The same package-sale block as the front desk's cart (EM 2026-11-07): one form for selling a
// package everywhere, "Mua cho người khác" included.
import {
  PackageSales,
  checkoutProblem,
  newSale,
  saleErrorOf,
  sellPackageOnly,
} from "../front-desk/CartPackages";
import { PaymentChoices } from "../front-desk/CartOptions";
import {
  type CartLine,
  type PackageSaleDraft,
  packagesDue,
} from "../front-desk/api";
import "../front-desk/front-desk.css";
import {
  type PaymentMode,
  activeStaffQuery,
  parseAmount,
  paymentsFor,
} from "./api";
import { boughtForOtherText } from "./saleText";

// A profile sale has no visit: no cart lines, nothing covered today, no technician to default to.
// `context` is its own empty list (not `NO_LINES`), so the cart-only hint about adding services
// stays hidden.
const NO_LINES: CartLine[] = [];
const NO_CONTEXT: { serviceId: string; staffId: string }[] = [];
const NOTHING_COVERED: ReadonlyMap<number, string> = new Map();

/** What a successful sale tells the desk: who got the package, and who paid when it is a gift. */
export function soldText(
  sale: Sale,
  draft: PackageSaleDraft,
  payerName: string,
): string {
  const p = sale.customer_packages[0];
  if (draft.beneficiary)
    return boughtForOtherText(
      p?.name ?? draft.plan?.name ?? "gói liệu trình",
      draft.beneficiary.full_name,
      payerName,
    );
  return p
    ? `Đã bán "${p.name}" — còn ${String(p.sessions_remaining)}/${String(p.sessions_total)} buổi.`
    : "Đã lưu hóa đơn.";
}

/**
 * US3: sell a package from the customer profile with the front desk's package block (plan, closer
 * or "Khách tự chọn", gift, discount within the manager cap, "Mua cho người khác") and its payment
 * choices. Totals are previews; the server prices the sale.
 */
export function SellPackageDialog({
  customerId,
  onDone,
  onCancel,
}: {
  customerId: string;
  /** `message` is the confirmation toast. */
  onDone: (message: string) => void;
  onCancel: () => void;
}) {
  const queryClient = useQueryClient();
  const id = useId();
  const { data: me } = useSuspenseQuery(meQuery);
  const staff = useQuery(activeStaffQuery);
  const customer = useQuery(customerQuery(customerId));
  // One key for the life of this dialog: double submit or retry = the same sale (AC3.7).
  const [idempotencyKey] = useState(() => crypto.randomUUID());
  const [sales, setSales] = useState<PackageSaleDraft[]>(() => [newSale([])]);
  const [mode, setMode] = useState<PaymentMode>("cash");
  const [cashText, setCashText] = useState("");
  const [attempted, setAttempted] = useState(false);
  const onSales = useCallback(
    (update: (s: PackageSaleDraft[]) => PackageSaleDraft[]) => {
      setSales(update);
    },
    [],
  );
  const onOpened = useCallback(() => undefined, []);

  const unlimited = can(me, "sale.discount_unlimited");
  const total = packagesDue(sales);
  const cashPart = parseAmount(cashText);
  const cashProblem =
    total > 0 &&
    mode === "mixed" &&
    (Number.isNaN(cashPart) || cashPart <= 0 || cashPart >= total);

  const mutation = useMutation({
    mutationFn: ({
      sale,
      payments,
    }: {
      sale: PackageSaleDraft;
      payments: PaymentCreate[];
    }) => sellPackageOnly(customerId, sale, payments, idempotencyKey),
    onSuccess: async (result, { sale }) => {
      // The payer's profile and, for a gift, the beneficiary's: both package lists changed.
      const owners = [customerId, sale.beneficiary?.id].filter(
        (x): x is string => x !== undefined,
      );
      await Promise.all(
        owners.map((owner) =>
          queryClient.invalidateQueries({
            queryKey: ["customer-packages", owner],
          }),
        ),
      );
      for (const owner of owners)
        void invalidateManagementViews(queryClient, owner);
      onDone(soldText(result, sale, customer.data?.full_name ?? "khách"));
    },
  });

  // Each block shows its own problem once a save was tried; only "no block at all" is shown here.
  const save = () => {
    setAttempted(true);
    const first = sales[0];
    if (
      !first ||
      checkoutProblem(sales, 0, false, unlimited) ||
      cashProblem ||
      mutation.isPending
    )
      return;
    mutation.mutate({
      sale: first,
      payments: paymentsFor(total, mode, cashPart),
    });
  };
  const generalError =
    mutation.isError && !saleErrorOf(mutation.error) ? mutation.error : null;

  return (
    // Hosted by a BottomSheet (CustomerPackages): the sheet is the dialog and carries the title.
    <div className="grid content-start gap-4">
      {staff.isError && (
        <p role="alert" className="alert">
          {messageFor(staff.error)}
        </p>
      )}
      <PackageSales
        sales={sales}
        onSales={onSales}
        lines={NO_LINES}
        context={NO_CONTEXT}
        covered={NOTHING_COVERED}
        maxSales={1}
        titleId={`${id}-packages`}
        title="Gói liệu trình"
        staff={staff.data ?? []}
        customerId={customerId}
        canDiscount={can(me, "sale.discount") || unlimited}
        unlimitedDiscount={unlimited}
        attempted={attempted}
        serverError={mutation.error}
        openPlanId={null}
        onOpened={onOpened}
      />
      {total > 0 && (
        <PaymentChoices
          name={`${id}-payment`}
          mode={mode}
          onMode={setMode}
          cashText={cashText}
          onCashText={setCashText}
          total={total}
          invalid={attempted && cashProblem}
        />
      )}
      {attempted && cashProblem && (
        <p role="alert" className="field-error">
          Tiền mặt phải lớn hơn 0 và nhỏ hơn số phải trả.
        </p>
      )}
      {attempted && sales.length === 0 && (
        <p role="alert" className="field-error">
          Bấm “Bán gói liệu trình” để chọn gói.
        </p>
      )}
      <p className="form-total" aria-live="polite">
        Phải trả: {formatVnd(total)}
      </p>
      {generalError !== null && (
        <p role="alert" className="alert">
          {messageFor(generalError)}
          {fieldMessage(generalError, "lines[0].package_plan_id")
            ? " Gói này đã ngừng bán."
            : ""}
        </p>
      )}
      <StickyActions>
        <button type="button" onClick={onCancel} className="btn btn-secondary">
          Hủy
        </button>
        <button
          type="button"
          onClick={save}
          aria-busy={mutation.isPending || undefined}
          className="btn btn-primary"
        >
          Lưu & thu tiền
        </button>
      </StickyActions>
    </div>
  );
}
