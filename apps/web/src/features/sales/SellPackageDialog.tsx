import type { PackagePlan, Sale } from "@vpyd/api-client";
import {
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import { useState } from "react";
import { StickyActions } from "../../components/StickyActions";
import { fieldMessage, messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { plansQuery } from "../catalog/api";
import {
  type PaymentMode,
  activeStaffQuery,
  createSale,
  exceedsCap,
  parseAmount,
  paymentsFor,
} from "./api";
import { invalidateManagementViews } from "../../lib/invalidate";

const input =
  "min-h-11 w-full rounded-[6px] border border-border bg-surface px-3";

function sessionsOf(plan: PackagePlan): string {
  const paid = plan.items.reduce((n, i) => n + i.sessions, 0);
  const bonus = plan.items.reduce((n, i) => n + i.bonus_sessions, 0);
  const validity = plan.validity_months
    ? ` · HSD ${String(plan.validity_months)} tháng`
    : "";
  return `${String(paid)} buổi${bonus ? ` + tặng ${String(bonus)}` : ""}${validity}`;
}

/** US3: sell a package from the customer card. Totals shown here are previews; the server prices the sale. */
export function SellPackageDialog({
  customerId,
  onDone,
  onCancel,
}: {
  customerId: string;
  onDone: (sale: Sale) => void;
  onCancel: () => void;
}) {
  const queryClient = useQueryClient();
  const { data: me } = useSuspenseQuery(meQuery);
  const plans = useQuery(plansQuery(false));
  const staff = useQuery(activeStaffQuery);
  // One key for the life of this dialog: double submit or retry = the same sale (AC3.7).
  const [idempotencyKey] = useState(() => crypto.randomUUID());

  const [planId, setPlanId] = useState("");
  const [giftNote, setGiftNote] = useState("");
  const [closerId, setCloserId] = useState("");
  const [discountText, setDiscountText] = useState("");
  const [reason, setReason] = useState("");
  const [mode, setMode] = useState<PaymentMode>("cash");
  const [cashText, setCashText] = useState("");
  const [touched, setTouched] = useState(false);

  const canDiscount =
    can(me, "sale.discount") || can(me, "sale.discount_unlimited");
  const plan = plans.data?.find((p) => p.id === planId);
  const subtotal = plan?.price ?? 0;
  const discount = discountText.trim() === "" ? 0 : parseAmount(discountText);
  const total = Number.isNaN(discount)
    ? subtotal
    : Math.max(subtotal - discount, 0);
  const cashPart = parseAmount(cashText);

  const problems = {
    plan: plan ? undefined : "Chọn gói liệu trình.",
    closer: closerId ? undefined : "Chọn người tư vấn chốt.",
    discount: Number.isNaN(discount)
      ? "Nhập số tiền (chỉ chữ số)."
      : discount > subtotal
        ? "Giảm giá lớn hơn giá gói."
        : exceedsCap(subtotal, discount, can(me, "sale.discount_unlimited"))
          ? "Vượt 10% — cần chủ spa duyệt."
          : undefined,
    reason:
      discount > 0 && reason.trim() === ""
        ? "Giảm giá phải ghi lý do."
        : undefined,
    cash:
      mode === "mixed" &&
      (Number.isNaN(cashPart) || cashPart <= 0 || cashPart >= total)
        ? "Tiền mặt phải lớn hơn 0 và nhỏ hơn số phải trả."
        : undefined,
  };
  const valid = Object.values(problems).every((p) => p === undefined);

  const mutation = useMutation({
    mutationFn: () =>
      createSale(
        {
          customer_id: customerId,
          closer_staff_id: closerId,
          lines: [{ package_plan_id: planId, gift_note: giftNote.trim() }],
          discount:
            discount > 0 ? { amount: discount, reason: reason.trim() } : null,
          payments: paymentsFor(total, mode, cashPart),
        },
        idempotencyKey,
      ),
    onSuccess: async (sale) => {
      await queryClient.invalidateQueries({
        queryKey: ["customer-packages", customerId],
      });
      void invalidateManagementViews(queryClient, customerId);
      onDone(sale);
    },
  });

  const show = (message: string | undefined) =>
    touched && message ? (
      <span className="text-sm text-danger">{message}</span>
    ) : null;

  return (
    // Hosted by a BottomSheet (CustomerPackages): the sheet is the dialog and carries the title.
    <div className="grid content-start gap-3">
      {(plans.isError || staff.isError) && (
        <p role="alert" className="text-danger">
          {messageFor(plans.error ?? staff.error)}
        </p>
      )}
      <form
        noValidate
        aria-label="Bán gói liệu trình"
        onSubmit={(e) => {
          e.preventDefault();
          setTouched(true);
          if (valid && !mutation.isPending) mutation.mutate();
        }}
        className="grid gap-3 sm:grid-cols-2"
      >
        <label className="block sm:col-span-2">
          <span className="text-sm font-semibold">Gói liệu trình</span>
          <select
            className={input}
            value={planId}
            onChange={(e) => {
              setPlanId(e.target.value);
              setGiftNote(
                plans.data?.find((p) => p.id === e.target.value)?.gift_note ??
                  "",
              );
            }}
          >
            <option value="">
              {plans.isPending ? "Đang tải gói…" : "Chọn gói…"}
            </option>
            {plans.data?.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name} — {formatVnd(p.price)} ({sessionsOf(p)})
              </option>
            ))}
          </select>
          {show(problems.plan)}
        </label>
        <label className="block">
          <span className="text-sm font-semibold">Quà tặng hiện vật</span>
          <input
            className={input}
            value={giftNote}
            onChange={(e) => {
              setGiftNote(e.target.value);
            }}
          />
        </label>
        <label className="block">
          <span className="text-sm font-semibold">Người tư vấn chốt</span>
          <select
            className={input}
            value={closerId}
            onChange={(e) => {
              setCloserId(e.target.value);
            }}
          >
            <option value="">
              {staff.isPending ? "Đang tải…" : "Chọn nhân viên…"}
            </option>
            {staff.data?.map((s) => (
              <option key={s.id} value={s.id}>
                {s.full_name} ({s.position})
              </option>
            ))}
          </select>
          {show(problems.closer)}
        </label>

        {canDiscount && (
          <>
            <label className="block">
              <span className="text-sm font-semibold">
                Giảm giá (đ, không bắt buộc)
              </span>
              <input
                className={input}
                inputMode="numeric"
                value={discountText}
                onChange={(e) => {
                  setDiscountText(e.target.value);
                }}
              />
              {discount > 0 && subtotal > 0 && !Number.isNaN(discount) && (
                <span className="block text-xs text-text-muted">
                  ≈ {String(Math.round((discount * 1000) / subtotal) / 10)}% giá
                  gói
                </span>
              )}
              {show(problems.discount) ??
                (exceedsCap(
                  subtotal,
                  discount,
                  can(me, "sale.discount_unlimited"),
                ) && (
                  <span className="text-sm text-danger">
                    Vượt 10% — cần chủ spa duyệt.
                  </span>
                ))}
            </label>
            <label className="block">
              <span className="text-sm font-semibold">Lý do giảm giá</span>
              <input
                className={input}
                value={reason}
                onChange={(e) => {
                  setReason(e.target.value);
                }}
              />
              {show(problems.reason)}
            </label>
          </>
        )}

        <fieldset className="grid content-start gap-2 sm:col-span-2">
          <legend className="text-sm font-semibold">Thanh toán (thu đủ)</legend>
          <div className="flex flex-wrap gap-3">
            {(
              [
                ["cash", "Tiền mặt"],
                ["bank_transfer", "Chuyển khoản"],
                ["mixed", "Tiền mặt + chuyển khoản"],
              ] as const
            ).map(([value, label]) => (
              <label
                key={value}
                className="inline-flex min-h-11 items-center gap-2"
              >
                <input
                  type="radio"
                  name="payment-mode"
                  checked={mode === value}
                  onChange={() => {
                    setMode(value);
                  }}
                />
                {label}
              </label>
            ))}
          </div>
          {mode === "mixed" && (
            <label className="block max-w-xs">
              <span className="text-sm font-semibold">Tiền mặt (đ)</span>
              <input
                className={input}
                inputMode="numeric"
                value={cashText}
                onChange={(e) => {
                  setCashText(e.target.value);
                }}
              />
              {!Number.isNaN(cashPart) && cashPart > 0 && cashPart < total && (
                <span className="block text-xs text-text-muted">
                  Chuyển khoản: {formatVnd(total - cashPart)}
                </span>
              )}
              {show(problems.cash)}
            </label>
          )}
        </fieldset>

        <p className="text-lg font-semibold sm:col-span-2" aria-live="polite">
          Phải trả: {formatVnd(total)}
        </p>
        {mutation.isError && (
          <p
            role="alert"
            className="rounded-[10px] bg-danger-soft p-3 text-sm text-danger sm:col-span-2"
          >
            {messageFor(mutation.error)}
            {fieldMessage(mutation.error, "lines[0].package_plan_id")
              ? " Gói này đã ngừng bán."
              : ""}
          </p>
        )}
        <StickyActions className="sm:col-span-2">
          <button type="button" onClick={onCancel} className="btn btn-secondary">
            Hủy
          </button>
          <button type="submit" disabled={mutation.isPending} className="btn btn-primary">
            {mutation.isPending ? "Đang lưu…" : "Lưu & thu tiền"}
          </button>
        </StickyActions>
      </form>
    </div>
  );
}
