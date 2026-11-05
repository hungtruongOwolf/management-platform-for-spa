import type { CustomerPackage } from "@vpyd/api-client";
import { useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import { useEffect, useId, useRef, useState } from "react";
import { fieldMessage, messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { formatDate, parseAmount } from "../sales/api";
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

const input =
  "min-h-11 w-full rounded-[6px] border border-border bg-surface px-3";
const secondary =
  "min-h-11 rounded-[10px] border border-border px-3 text-sm font-medium";

type Open = "extend" | "cancel" | "void" | null;

/**
 * Lifecycle actions on one package of the customer profile (US6, US7), shown only to the roles
 * that hold the permission (UX only — the server re-checks): extend (owner), cancel with refund
 * (owner), void the sale that created it while unused (manager or owner).
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
  if (pkg.status === "cancelled") return null;

  const canExtend = can(me, "package.extend_expiry") && pkg.expires_on != null;
  const canCancel = can(me, "package.cancel_refund");
  const canVoidSale =
    can(me, "record.void_same_day") &&
    pkg.sale_id != null &&
    isUnused(pkg);
  if (!canExtend && !canCancel && !canVoidSale) return null;

  const finish = (message: string) => {
    setOpen(null);
    void queryClient.invalidateQueries({ queryKey: ["customer-packages"] });
    void queryClient.invalidateQueries({ queryKey: ["visits"] });
    onDone(message);
  };

  return (
    <div className="w-full">
      {open === null && (
        <div className="flex flex-wrap gap-2 pt-1">
          {canExtend && (
            <button
              type="button"
              className={secondary}
              onClick={() => {
                setOpen("extend");
              }}
            >
              Gia hạn
            </button>
          )}
          {canCancel && (
            <button
              type="button"
              className={secondary}
              onClick={() => {
                setOpen("cancel");
              }}
            >
              Hủy liệu trình + hoàn tiền
            </button>
          )}
          {canVoidSale && (
            <button
              type="button"
              className={secondary}
              onClick={() => {
                setOpen("void");
              }}
            >
              Hủy hóa đơn bán gói
            </button>
          )}
        </div>
      )}
      {open === "extend" && (
        <ExtendForm
          pkg={pkg}
          onCancel={() => {
            setOpen(null);
          }}
          onDone={(p) => {
            finish(
              `Đã gia hạn "${p.name}" tới ${formatDate(p.expires_on ?? "")}.`,
            );
          }}
        />
      )}
      {open === "cancel" && (
        <CancelForm
          pkg={pkg}
          onCancel={() => {
            setOpen(null);
          }}
          onDone={(p) => {
            finish(
              p.refund
                ? `Đã hủy "${p.name}" và hoàn ${formatVnd(p.refund.amount)}.`
                : `Đã hủy "${p.name}".`,
            );
          }}
        />
      )}
      {open === "void" && pkg.sale_id != null && (
        <VoidForm
          title={`Hủy hóa đơn bán "${pkg.name}"`}
          consequence="Liệu trình chưa dùng buổi nào sẽ bị hủy; tiền đã thu được ghi bút toán đảo (không xóa)."
          confirmLabel="Hủy hóa đơn"
          run={(reason, key) => voidSale(pkg.sale_id ?? "", reason, key)}
          onCancel={() => {
            setOpen(null);
          }}
          onDone={() => {
            finish(`Đã hủy hóa đơn bán "${pkg.name}".`);
          }}
        />
      )}
    </div>
  );
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
  const dateRef = useRef<HTMLInputElement>(null);
  const [idempotencyKey] = useState(() => crypto.randomUUID());
  const today = spaToday();
  const base =
    pkg.expires_on && pkg.expires_on > today ? pkg.expires_on : today;
  const [expiresOn, setExpiresOn] = useState(() => addMonths(base, 1));
  const [reason, setReason] = useState("");
  const [touched, setTouched] = useState(false);
  const action = useFormAction({
    run: () =>
      extendPackage(
        pkg,
        { expires_on: expiresOn, reason: reason.trim() },
        idempotencyKey,
      ),
    onSuccess: onDone,
  });
  useEffect(() => {
    dateRef.current?.focus();
  }, []);

  const reasonProblem =
    (touched && reason.trim() === "" ? "Bắt buộc ghi lý do." : undefined) ??
    fieldMessage(action.error, "reason");
  const dateProblem =
    (touched && pkg.expires_on && expiresOn <= pkg.expires_on
      ? "Hạn mới phải sau hạn hiện tại."
      : undefined) ?? fieldMessage(action.error, "expires_on");

  return (
    <section
      role="dialog"
      aria-modal="false"
      aria-labelledby={titleId}
      onKeyDown={(e) => {
        if (e.key === "Escape") onCancel();
      }}
      className="mt-2 space-y-2 rounded-[10px] border border-primary bg-primary-soft/40 p-3"
    >
      <h3 id={titleId} className="font-semibold">
        Gia hạn "{pkg.name}"
      </h3>
      <p className="text-sm text-text-muted">
        Hạn hiện tại:{" "}
        {pkg.expires_on ? formatDate(pkg.expires_on) : "không hạn"} · còn{" "}
        {String(pkg.sessions_remaining)}/{String(pkg.sessions_total)} buổi
      </p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          setTouched(true);
          if (
            reason.trim() !== "" &&
            !(pkg.expires_on && expiresOn <= pkg.expires_on)
          ) {
            action.submit();
          }
        }}
        className="space-y-2"
      >
        <label className="block">
          <span className="text-sm font-medium">Hạn mới</span>
          <input
            ref={dateRef}
            type="date"
            value={expiresOn}
            min={today}
            onChange={(e) => {
              setExpiresOn(e.target.value);
            }}
            aria-invalid={dateProblem ? true : undefined}
            className={input}
          />
        </label>
        <div
          className="flex flex-wrap gap-2"
          aria-label="Gia hạn nhanh"
          role="group"
        >
          {[1, 2, 3, 6].map((m) => (
            <button
              key={m}
              type="button"
              className={secondary}
              onClick={() => {
                setExpiresOn(addMonths(base, m));
              }}
            >
              +{m} tháng
            </button>
          ))}
        </div>
        {dateProblem && <p className="text-sm text-danger">{dateProblem}</p>}
        <label className="block">
          <span className="text-sm font-medium">Lý do gia hạn</span>
          <textarea
            value={reason}
            maxLength={500}
            rows={2}
            aria-invalid={reasonProblem ? true : undefined}
            onChange={(e) => {
              setReason(e.target.value);
            }}
            className="w-full rounded-[6px] border border-border bg-surface px-3 py-2"
          />
        </label>
        {reasonProblem && (
          <p className="text-sm text-danger">{reasonProblem}</p>
        )}
        {action.error != null &&
          !fieldMessage(action.error, "reason") &&
          !fieldMessage(action.error, "expires_on") && (
            <p role="alert" className="text-sm text-danger">
              {messageFor(action.error)}
            </p>
          )}
        <div className="flex gap-2">
          <button
            type="submit"
            disabled={action.pending}
            className="min-h-11 rounded-[10px] bg-primary px-4 font-semibold text-on-primary disabled:opacity-60"
          >
            {action.pending ? "Đang lưu…" : "Gia hạn"}
          </button>
          <button type="button" onClick={onCancel} className={secondary}>
            Thôi
          </button>
        </div>
      </form>
    </section>
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
          ? {
              reason: reason.trim(),
              refund_amount: amount,
              refund_method: method,
            }
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
  const reasonProblem =
    (touched && reason.trim() === "" ? "Bắt buộc ghi lý do." : undefined) ??
    fieldMessage(action.error, "reason");

  return (
    <section
      role="dialog"
      aria-modal="false"
      aria-labelledby={titleId}
      onKeyDown={(e) => {
        if (e.key === "Escape") onCancel();
      }}
      className="mt-2 space-y-2 rounded-[10px] border border-danger/50 bg-surface p-3"
    >
      <h3 id={titleId} className="font-semibold">
        Hủy liệu trình "{pkg.name}"
      </h3>
      <p className="text-sm text-text-muted">
        Còn {String(pkg.sessions_remaining)}/{String(pkg.sessions_total)} buổi
        sẽ về 0. Khách đã trả {formatVnd(pkg.sold_price)}. Khoản hoàn ghi vào sổ
        quỹ hôm nay.
      </p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          setTouched(true);
          if (reason.trim() !== "" && !amountProblem) action.submit();
        }}
        className="space-y-2"
      >
        <label className="block">
          <span className="text-sm font-medium">
            Số tiền hoàn (để trống = không hoàn)
          </span>
          <input
            ref={amountRef}
            inputMode="numeric"
            value={amountText}
            onChange={(e) => {
              setAmountText(e.target.value);
            }}
            aria-invalid={amountProblem ? true : undefined}
            className={input}
          />
        </label>
        {amountProblem && (
          <p className="text-sm text-danger">{amountProblem}</p>
        )}
        {amount > 0 && (
          <fieldset className="flex flex-wrap gap-3">
            <legend className="text-sm font-medium">Hoàn bằng</legend>
            {(
              [
                ["cash", "Tiền mặt"],
                ["bank_transfer", "Chuyển khoản"],
              ] as const
            ).map(([value, label]) => (
              <label key={value} className="flex min-h-11 items-center gap-2">
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
          </fieldset>
        )}
        <label className="block">
          <span className="text-sm font-medium">Lý do hủy</span>
          <textarea
            value={reason}
            maxLength={500}
            rows={2}
            aria-invalid={reasonProblem ? true : undefined}
            onChange={(e) => {
              setReason(e.target.value);
            }}
            className="w-full rounded-[6px] border border-border bg-surface px-3 py-2"
          />
        </label>
        {reasonProblem && (
          <p className="text-sm text-danger">{reasonProblem}</p>
        )}
        {action.error != null && !fieldMessage(action.error, "reason") && (
          <p role="alert" className="text-sm text-danger">
            {messageFor(action.error)}
          </p>
        )}
        <div className="flex gap-2">
          <button
            type="submit"
            disabled={action.pending}
            className="min-h-11 rounded-[10px] bg-danger px-4 font-semibold text-on-primary disabled:opacity-60"
          >
            {action.pending
              ? "Đang hủy…"
              : amount > 0
                ? `Hủy và hoàn ${formatVnd(amount)}`
                : "Hủy liệu trình"}
          </button>
          <button type="button" onClick={onCancel} className={secondary}>
            Thôi
          </button>
        </div>
      </form>
    </section>
  );
}
