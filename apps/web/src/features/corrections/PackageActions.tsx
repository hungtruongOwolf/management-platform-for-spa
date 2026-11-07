import type { CustomerPackage } from "@vpyd/api-client";
import { useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import { useEffect, useId, useRef, useState } from "react";
import { DateField } from "../../components/DateField";
import { RowMenu, type RowMenuItem } from "../../components/RowMenu";
import { fieldMessage, messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { formatDate, parseAmount } from "../sales/api";
import { isClosedPackage } from "../sales/packageState";
import {
  addMonths,
  cancelPackage,
  extendPackage,
  isUnused,
  spaToday,
  voidSale,
} from "./api";
import { useFormAction } from "./useFormAction";
import { VoidForm } from "./VoidForm";
import { invalidateManagementViews } from "../../lib/invalidate";

type Open = "extend" | "cancel" | "void" | null;

const ACTION_LABEL: Record<Exclude<Open, null>, string> = {
  extend: "Gia hạn",
  cancel: "Hủy gói và hoàn tiền",
  void: "Hủy hóa đơn bán gói",
};

/** The actions this caller may take on this package, in menu order (UX only — the server re-checks). */
export function packageActionsFor(
  pkg: CustomerPackage,
  permissions: { extend: boolean; cancel: boolean; voidSale: boolean },
): Exclude<Open, null>[] {
  // A used-up or cancelled package is closed: nothing to extend, cancel or void (one state, and
  // never an action that state forbids — design-system.md "Quy tắc" 7).
  if (isClosedPackage(pkg)) return [];
  const out: Exclude<Open, null>[] = [];
  if (permissions.extend && pkg.expires_on != null) out.push("extend");
  if (permissions.cancel) out.push("cancel");
  if (permissions.voidSale && pkg.sale_id != null && isUnused(pkg)) out.push("void");
  return out;
}

/**
 * Lifecycle actions on one open package of the customer profile (US6, US7): extend (owner),
 * cancel with refund (owner), void the sale that created it while unused (manager or owner). One
 * action is a ghost button; two or more sit in the row's "⋯" menu (rule 8). The chosen form opens
 * under the row. Renders `display: contents` so the trigger and the form take their own grid
 * cells of the package row.
 */
export function PackageActions({
  pkg,
  onDone,
}: {
  pkg: CustomerPackage;
  onDone: (message: string) => void;
}) {
  const { data: me } = useSuspenseQuery(meQuery);
  const queryClient = useQueryClient();
  const [open, setOpen] = useState<Open>(null);
  const actions = packageActionsFor(pkg, {
    extend: can(me, "package.extend_expiry"),
    cancel: can(me, "package.cancel_refund"),
    voidSale: can(me, "record.void_same_day"),
  });
  if (actions.length === 0) return null;

  const finish = (message: string) => {
    setOpen(null);
    void queryClient.invalidateQueries({ queryKey: ["customer-packages"] });
    void queryClient.invalidateQueries({ queryKey: ["visits"] });
    void invalidateManagementViews(queryClient, pkg.customer_id);
    onDone(message);
  };
  const close = () => {
    setOpen(null);
  };
  const [only] = actions;

  return (
    <div className="pkg-actions">
      <div className="pkg-actions-trigger">
        {actions.length === 1 && only ? (
          <button
            type="button"
            className="btn btn-ghost"
            aria-expanded={open === only}
            onClick={() => {
              setOpen(open === only ? null : only);
            }}
          >
            {ACTION_LABEL[only]}
          </button>
        ) : (
          <RowMenu
            label={`Thao tác gói ${pkg.name}`}
            items={actions.map(
              (a): RowMenuItem => ({
                label: ACTION_LABEL[a],
                danger: a !== "extend",
                onSelect: () => {
                  setOpen(a);
                },
              }),
            )}
          />
        )}
      </div>
      {open === "extend" && (
        <ExtendForm
          pkg={pkg}
          onCancel={close}
          onDone={(p) => {
            finish(`Đã gia hạn "${p.name}" tới ${formatDate(p.expires_on ?? "")}.`);
          }}
        />
      )}
      {open === "cancel" && (
        <CancelForm
          pkg={pkg}
          onCancel={close}
          onDone={(p) => {
            finish(p.refund ? `Đã hủy "${p.name}" và hoàn ${formatVnd(p.refund.amount)}.` : `Đã hủy "${p.name}".`);
          }}
        />
      )}
      {open === "void" && pkg.sale_id != null && (
        <div className="pkg-actions-form">
          <VoidForm
            title={`Hủy hóa đơn bán "${pkg.name}"`}
            consequence="Liệu trình chưa dùng buổi nào sẽ bị hủy; tiền đã thu được ghi bút toán đảo (không xóa)."
            confirmLabel="Hủy hóa đơn"
            run={(reason, key) => voidSale(pkg.sale_id ?? "", reason, key)}
            onCancel={close}
            onDone={() => {
              finish(`Đã hủy hóa đơn bán "${pkg.name}".`);
            }}
          />
        </div>
      )}
    </div>
  );
}

/**
 * The cancel button names the refund only when that amount would really be sent: a number,
 * above 0 and within what the customer paid. An empty field cancels without a refund; anything
 * else (not a number, over the maximum) gets a neutral label while the field shows the error.
 */
export function cancelLabel(amountText: string, refundableMax: number): string {
  const amount = amountText.trim() === "" ? 0 : parseAmount(amountText);
  if (amount === 0) return "Hủy gói";
  return !Number.isNaN(amount) && amount <= refundableMax
    ? `Hủy và hoàn ${formatVnd(amount)}`
    : "Hủy gói và hoàn tiền";
}

/** Field with its label above and its error below, wired with aria-describedby. */
function useFieldIds() {
  const id = useId();
  return { input: `${id}-input`, error: `${id}-error`, hint: `${id}-hint` };
}

function ExtendForm({
  pkg,
  onDone,
  onCancel,
}: {
  pkg: CustomerPackage;
  onDone: (p: CustomerPackage) => void;
  onCancel: () => void;
}) {
  const titleId = useId();
  const reasonIds = useFieldIds();
  const dateIds = useFieldIds();
  const dateRef = useRef<HTMLDivElement>(null);
  const [idempotencyKey] = useState(() => crypto.randomUUID());
  const today = spaToday();
  const base = pkg.expires_on && pkg.expires_on > today ? pkg.expires_on : today;
  const [expiresOn, setExpiresOn] = useState(() => addMonths(base, 1));
  const [reason, setReason] = useState("");
  const [touched, setTouched] = useState(false);
  // Extending changes what the customer is owed: the owner reviews old → new expiry first.
  const [confirming, setConfirming] = useState(false);
  const action = useFormAction({
    run: () => extendPackage(pkg, { expires_on: expiresOn, reason: reason.trim() }, idempotencyKey),
    onSuccess: onDone,
  });
  useEffect(() => {
    if (!confirming) dateRef.current?.querySelector<HTMLInputElement>("input[type=text]")?.focus();
  }, [confirming]);

  const reasonProblem = (touched && reason.trim() === "" ? "Bắt buộc ghi lý do." : undefined) ?? fieldMessage(action.error, "reason");
  const dateProblem =
    (touched && pkg.expires_on && expiresOn <= pkg.expires_on ? "Hạn mới phải sau hạn hiện tại." : undefined) ??
    fieldMessage(action.error, "expires_on");

  return (
    <section
      role="dialog"
      aria-modal="false"
      aria-labelledby={titleId}
      onKeyDown={(e) => {
        if (e.key === "Escape") onCancel();
      }}
      className="pkg-actions-form row-form"
    >
      <div>
        <h3 id={titleId} className="panel-title">
          Gia hạn "{pkg.name}"
        </h3>
        <p className="row-meta">
          Hạn hiện tại: {pkg.expires_on ? formatDate(pkg.expires_on) : "không hạn"} · còn {String(pkg.sessions_remaining)}/
          {String(pkg.sessions_total)} buổi
        </p>
      </div>
      {confirming ? (
        <ExtendReview
          oldExpiry={pkg.expires_on}
          newExpiry={expiresOn}
          reason={reason.trim()}
          pending={action.pending}
          error={action.error}
          onConfirm={action.submit}
          onEdit={() => {
            setConfirming(false);
          }}
        />
      ) : (
        <form
          noValidate
          onSubmit={(e) => {
            e.preventDefault();
            setTouched(true);
            if (reason.trim() !== "" && !(pkg.expires_on && expiresOn <= pkg.expires_on)) setConfirming(true);
          }}
          className="form-grid"
        >
          <div ref={dateRef} className="field">
            <DateField
              label="Hạn mới"
              value={expiresOn}
              min={today}
              invalid={Boolean(dateProblem)}
              {...(dateProblem ? { describedBy: dateIds.error } : {})}
              onChange={(iso) => {
                if (iso) setExpiresOn(iso);
              }}
            />
            {dateProblem && (
              <span id={dateIds.error} className="field-error">
                {dateProblem}
              </span>
            )}
          </div>
          <div className="field">
            <span aria-hidden="true">Gia hạn nhanh</span>
            <div className="choice-row" aria-label="Gia hạn nhanh" role="group">
              {[1, 2, 3, 6].map((m) => (
                <button
                  key={m}
                  type="button"
                  className="btn btn-secondary"
                  onClick={() => {
                    setExpiresOn(addMonths(base, m));
                  }}
                >
                  +{m} tháng
                </button>
              ))}
            </div>
          </div>
          <div className="field span-2">
            <label htmlFor={reasonIds.input}>Lý do gia hạn</label>
            <textarea
              id={reasonIds.input}
              value={reason}
              maxLength={500}
              rows={2}
              aria-invalid={reasonProblem ? true : undefined}
              aria-describedby={reasonProblem ? reasonIds.error : undefined}
              onChange={(e) => {
                setReason(e.target.value);
              }}
              className="input"
            />
            {reasonProblem && (
              <span id={reasonIds.error} className="field-error">
                {reasonProblem}
              </span>
            )}
          </div>
          {action.error != null && !fieldMessage(action.error, "reason") && !fieldMessage(action.error, "expires_on") && (
            <p role="alert" className="alert span-2">
              {messageFor(action.error)}
            </p>
          )}
          <div className="button-row span-2">
            <button type="submit" className="btn btn-primary">
              Gia hạn
            </button>
            <button type="button" onClick={onCancel} className="btn btn-secondary">
              Bỏ qua
            </button>
          </div>
        </form>
      )}
    </section>
  );
}

/** The confirmation step of an extension: old → new expiry and the reason, before anything is saved. */
function ExtendReview({
  oldExpiry,
  newExpiry,
  reason,
  pending,
  error,
  onConfirm,
  onEdit,
}: {
  oldExpiry: string | null | undefined;
  newExpiry: string;
  reason: string;
  pending: boolean;
  error: unknown;
  onConfirm: () => void;
  onEdit: () => void;
}) {
  const confirmRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    confirmRef.current?.focus();
  }, []);
  return (
    <div role="group" aria-label="Xác nhận gia hạn" className="grid gap-3">
      <dl className="detail-grid">
        <div>
          <dt>Hạn cũ</dt>
          <dd>{oldExpiry ? formatDate(oldExpiry) : "không hạn"}</dd>
        </div>
        <div>
          <dt>Hạn mới</dt>
          <dd>
            <strong>{formatDate(newExpiry)}</strong>
          </dd>
        </div>
        <div>
          <dt>Lý do</dt>
          <dd>{reason}</dd>
        </div>
      </dl>
      {error != null && (
        <p role="alert" className="alert">
          {messageFor(error)}
        </p>
      )}
      <div className="button-row">
        <button
          ref={confirmRef}
          type="button"
          aria-busy={pending || undefined}
          onClick={() => {
            if (!pending) onConfirm();
          }}
          className="btn btn-primary"
        >
          Xác nhận gia hạn
        </button>
        <button type="button" onClick={onEdit} className="btn btn-secondary">
          Sửa lại
        </button>
      </div>
    </div>
  );
}

function CancelForm({
  pkg,
  onDone,
  onCancel,
}: {
  pkg: CustomerPackage;
  onDone: (p: CustomerPackage) => void;
  onCancel: () => void;
}) {
  const titleId = useId();
  const amountIds = useFieldIds();
  const reasonIds = useFieldIds();
  const amountRef = useRef<HTMLInputElement>(null);
  const [idempotencyKey] = useState(() => crypto.randomUUID());
  const [amountText, setAmountText] = useState("");
  const [method, setMethod] = useState<"cash" | "bank_transfer">("cash");
  const [reason, setReason] = useState("");
  const [touched, setTouched] = useState(false);
  const amount = amountText.trim() === "" ? 0 : parseAmount(amountText);
  const action = useFormAction({
    run: () =>
      cancelPackage(
        pkg.id,
        amount > 0
          ? { reason: reason.trim(), refund_amount: amount, refund_method: method }
          : { reason: reason.trim(), refund_amount: 0 },
        idempotencyKey,
      ),
    onSuccess: onDone,
  });
  useEffect(() => {
    amountRef.current?.focus();
  }, []);

  const amountProblem = Number.isNaN(amount)
    ? "Nhập số tiền (chỉ chữ số)."
    : amount > pkg.sold_price
      ? `Không hoàn quá số khách đã trả (${formatVnd(pkg.sold_price)}).`
      : undefined;
  const reasonProblem = (touched && reason.trim() === "" ? "Bắt buộc ghi lý do." : undefined) ?? fieldMessage(action.error, "reason");

  return (
    <section
      role="dialog"
      aria-modal="false"
      aria-labelledby={titleId}
      onKeyDown={(e) => {
        if (e.key === "Escape") onCancel();
      }}
      className="pkg-actions-form row-form"
    >
      <div>
        <h3 id={titleId} className="panel-title">
          Hủy gói "{pkg.name}"
        </h3>
        <p className="row-meta">
          Còn {String(pkg.sessions_remaining)}/{String(pkg.sessions_total)} buổi sẽ về 0. Khách đã trả {formatVnd(pkg.sold_price)}. Khoản hoàn ghi vào
          sổ quỹ hôm nay.
        </p>
      </div>
      <form
        noValidate
        onSubmit={(e) => {
          e.preventDefault();
          setTouched(true);
          if (!action.pending && reason.trim() !== "" && !amountProblem) action.submit();
        }}
        className="form-grid"
      >
        <div className="field">
          <label htmlFor={amountIds.input}>Số tiền hoàn (để trống = không hoàn)</label>
          <input
            id={amountIds.input}
            ref={amountRef}
            inputMode="numeric"
            value={amountText}
            onChange={(e) => {
              setAmountText(e.target.value);
            }}
            aria-invalid={amountProblem ? true : undefined}
            aria-describedby={amountProblem ? amountIds.error : amountIds.hint}
            className="input"
          />
          {amountProblem ? (
            <span id={amountIds.error} className="field-error">
              {amountProblem}
            </span>
          ) : (
            <span id={amountIds.hint} className="field-hint">
              Tối đa {formatVnd(pkg.sold_price)}
            </span>
          )}
        </div>
        {amount > 0 && (
          <fieldset>
            <legend>Hoàn bằng</legend>
            <div className="choice-row">
              {(
                [
                  ["cash", "Tiền mặt"],
                  ["bank_transfer", "Chuyển khoản"],
                ] as const
              ).map(([value, label]) => (
                <label key={value} className="choice">
                  <input
                    type="radio"
                    name={`${titleId}-method`}
                    checked={method === value}
                    onChange={() => {
                      setMethod(value);
                    }}
                  />
                  {label}
                </label>
              ))}
            </div>
          </fieldset>
        )}
        <div className="field span-2">
          <label htmlFor={reasonIds.input}>Lý do hủy</label>
          <textarea
            id={reasonIds.input}
            value={reason}
            maxLength={500}
            rows={2}
            aria-invalid={reasonProblem ? true : undefined}
            aria-describedby={reasonProblem ? reasonIds.error : undefined}
            onChange={(e) => {
              setReason(e.target.value);
            }}
            className="input"
          />
          {reasonProblem && (
            <span id={reasonIds.error} className="field-error">
              {reasonProblem}
            </span>
          )}
        </div>
        {action.error != null && !fieldMessage(action.error, "reason") && (
          <p role="alert" className="alert span-2">
            {messageFor(action.error)}
          </p>
        )}
        <div className="button-row span-2">
          <button type="submit" aria-busy={action.pending || undefined} className="btn btn-danger">
            {cancelLabel(amountText, pkg.sold_price)}
          </button>
          <button type="button" onClick={onCancel} className="btn btn-secondary">
            Bỏ qua
          </button>
        </div>
      </form>
    </section>
  );
}
