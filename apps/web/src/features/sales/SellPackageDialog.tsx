import type { PackagePlan, Sale } from "@vpyd/api-client";
import {
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import { useId, useState } from "react";
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

/** Select value for "Khách tự chọn": the customer chose the package, nobody is credited. */
const SELF_CHOSEN = "self";

function sessionsOf(plan: PackagePlan): string {
  const paid = plan.items.reduce((n, i) => n + i.sessions, 0);
  const bonus = plan.items.reduce((n, i) => n + i.bonus_sessions, 0);
  const validity = plan.validity_months
    ? ` · HSD ${String(plan.validity_months)} tháng`
    : " · không hạn";
  return `${String(paid)} buổi${bonus ? ` + tặng ${String(bonus)}` : ""}${validity}`;
}

/** US3: sell a package from the customer profile. Totals shown here are previews; the server prices the sale. */
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
  const id = useId();
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
  const overCap = exceedsCap(subtotal, discount, can(me, "sale.discount_unlimited"));

  const problems = {
    plan: plan ? undefined : "Chọn gói liệu trình.",
    closer: closerId
      ? undefined
      : "Chọn người tư vấn chốt (hoặc Khách tự chọn).",
    discount: Number.isNaN(discount)
      ? "Nhập số tiền (chỉ chữ số)."
      : discount > subtotal
        ? "Giảm giá lớn hơn giá gói."
        : overCap
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
          // "Khách tự chọn" credits nobody (EM 2026-11-06, Q-U2).
          closer_staff_id: closerId === SELF_CHOSEN ? null : closerId,
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

  type Key = keyof typeof problems;
  // The 10% cap shows as soon as it is crossed; every other problem after the first submit.
  const shown = (key: Key) =>
    key === "discount" && overCap ? problems.discount : touched ? problems[key] : undefined;
  const errorOf = (key: Key) => {
    const message = shown(key);
    return message ? (
      <span id={`${id}-${key}-error`} className="field-error">
        {message}
      </span>
    ) : null;
  };
  const described = (key: Key, hint?: string) =>
    [shown(key) ? `${id}-${key}-error` : undefined, hint].filter(Boolean).join(" ") || undefined;
  const invalid = (key: Key) => (shown(key) ? true : undefined);

  return (
    // Hosted by a BottomSheet (CustomerPackages): the sheet is the dialog and carries the title.
    <div className="grid content-start gap-4">
      {(plans.isError || staff.isError) && (
        <p role="alert" className="form-error">
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
        className="form-grid"
      >
        <div className="field span-2">
          <label htmlFor={`${id}-plan`}>Gói liệu trình</label>
          <select
            id={`${id}-plan`}
            className="input"
            value={planId}
            aria-invalid={invalid("plan")}
            aria-describedby={described("plan", plan ? `${id}-plan-hint` : undefined)}
            onChange={(e) => {
              setPlanId(e.target.value);
              setGiftNote(plans.data?.find((p) => p.id === e.target.value)?.gift_note ?? "");
            }}
          >
            <option value="">{plans.isPending ? "Đang tải gói…" : "Chọn gói…"}</option>
            {plans.data?.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name} — {formatVnd(p.price)}
              </option>
            ))}
          </select>
          {plan && (
            <span id={`${id}-plan-hint`} className="field-hint">
              {sessionsOf(plan)}
            </span>
          )}
          {errorOf("plan")}
        </div>
        <div className="field">
          <label htmlFor={`${id}-gift`}>Quà tặng hiện vật</label>
          <input
            id={`${id}-gift`}
            className="input"
            value={giftNote}
            placeholder="Không có"
            onChange={(e) => {
              setGiftNote(e.target.value);
            }}
          />
        </div>
        <div className="field">
          <label htmlFor={`${id}-closer`}>Người tư vấn chốt</label>
          <select
            id={`${id}-closer`}
            className="input"
            value={closerId}
            aria-invalid={invalid("closer")}
            aria-describedby={described("closer")}
            onChange={(e) => {
              setCloserId(e.target.value);
            }}
          >
            <option value="">{staff.isPending ? "Đang tải…" : "Chọn…"}</option>
            <option value={SELF_CHOSEN}>Khách tự chọn (không tính cho ai)</option>
            {staff.data?.map((s) => (
              <option key={s.id} value={s.id}>
                {s.full_name} ({s.position})
              </option>
            ))}
          </select>
          {errorOf("closer")}
        </div>

        {canDiscount && (
          <>
            <div className="field">
              <label htmlFor={`${id}-discount`}>Giảm giá (đ, không bắt buộc)</label>
              <input
                id={`${id}-discount`}
                className="input tabular"
                inputMode="numeric"
                value={discountText}
                aria-invalid={invalid("discount")}
                aria-describedby={described("discount", `${id}-discount-hint`)}
                onChange={(e) => {
                  setDiscountText(e.target.value);
                }}
              />
              <span id={`${id}-discount-hint`} className="field-hint">
                {discount > 0 && subtotal > 0 && !Number.isNaN(discount)
                  ? `≈ ${String(Math.round((discount * 1000) / subtotal) / 10)}% giá gói`
                  : "Quá 10% giá gói cần chủ spa duyệt."}
              </span>
              {errorOf("discount")}
            </div>
            <div className="field">
              <label htmlFor={`${id}-reason`}>Lý do giảm giá</label>
              <input
                id={`${id}-reason`}
                className="input"
                value={reason}
                aria-invalid={invalid("reason")}
                aria-describedby={described("reason")}
                onChange={(e) => {
                  setReason(e.target.value);
                }}
              />
              {errorOf("reason")}
            </div>
          </>
        )}

        <fieldset className="span-2">
          <legend>Thanh toán (thu đủ)</legend>
          <div className="choice-row">
            {(
              [
                ["cash", "Tiền mặt"],
                ["bank_transfer", "Chuyển khoản"],
                ["mixed", "Tiền mặt + chuyển khoản"],
              ] as const
            ).map(([value, label]) => (
              <label key={value} className="choice">
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
        </fieldset>
        {mode === "mixed" && (
          <div className="field">
            <label htmlFor={`${id}-cash`}>Tiền mặt (đ)</label>
            <input
              id={`${id}-cash`}
              className="input tabular"
              inputMode="numeric"
              value={cashText}
              aria-invalid={invalid("cash")}
              aria-describedby={described("cash", `${id}-cash-hint`)}
              onChange={(e) => {
                setCashText(e.target.value);
              }}
            />
            <span id={`${id}-cash-hint`} className="field-hint">
              {!Number.isNaN(cashPart) && cashPart > 0 && cashPart < total
                ? `Chuyển khoản: ${formatVnd(total - cashPart)}`
                : "Phần còn lại thu bằng chuyển khoản."}
            </span>
            {errorOf("cash")}
          </div>
        )}

        <p className="form-total span-2" aria-live="polite">
          Phải trả: {formatVnd(total)}
        </p>
        {mutation.isError && (
          <p role="alert" className="form-error span-2">
            {messageFor(mutation.error)}
            {fieldMessage(mutation.error, "lines[0].package_plan_id") ? " Gói này đã ngừng bán." : ""}
          </p>
        )}
        <StickyActions className="span-2">
          <button type="button" onClick={onCancel} className="btn btn-secondary">
            Hủy
          </button>
          <button type="submit" aria-busy={mutation.isPending || undefined} className="btn btn-primary">
            Lưu & thu tiền
          </button>
        </StickyActions>
      </form>
    </div>
  );
}
