// The cart's secondary pieces: the one-off service picker, and once a visit has lines the service
// time (giờ làm, owner back-dating) and the one-off discount. Each is one aligned row — label · current value · one ghost action — that
// opens its fields underneath. A lazy chunk prefetched after the first paint, off the startup bundle.
import type {
  Appointment,
  Customer,
  CustomerSummary,
  Service,
  Staff,
  Visit,
} from "@vpyd/api-client";
import { useQuery } from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import {
  type RefObject,
  Suspense,
  lazy,
  useEffect,
  useRef,
  useState,
} from "react";
import { Icon } from "../../app/Icon";
import { DateField } from "../../components/DateField";
import { TimeField } from "../../components/TimeField";
import { addMessages, messageFor } from "../../lib/errors";
import { serviceLabel } from "../../lib/names";
import { categoriesQuery, servicesQuery } from "../catalog/api";
import { customerQuery } from "../customers/api";
import { parseAmount } from "../sales/api";
import {
  type CartLine,
  type DiscountDraft,
  type Gender,
  type PaymentMode,
  discountPreview,
  priceFor,
  timeOf,
} from "./api";

// The service-time refusals only this chunk's time field leads to: kept off the startup bundle.
addMessages(
  {
    "visit.check_in_in_future": "Giờ làm không được ở tương lai.",
    "visit.check_in_too_old": "Chỉ nhập bù được trong vòng 1 năm.",
  },
  {},
);

const CustomerForm = lazy(() =>
  import("../customers/CustomerForm").then((m) => ({
    default: m.CustomerForm,
  })),
);

/**
 * "Thêm thông tin khách" (EM 2026-11-07): a nameless walk-in gets a name, phone, gender, birth date
 * or a note later, in place. The profile keeps its id, so the visit and the cart stay theirs
 * (ADR-0011: editing a nameless profile keeps its history).
 */
export function WalkInInfo({
  customerId,
  canWriteHealth,
  onSaved,
  onCancel,
}: {
  customerId: string;
  canWriteHealth: boolean;
  onSaved: (c: Customer) => void;
  onCancel: () => void;
}) {
  const customer = useQuery(customerQuery(customerId));
  const loading = (
    <p role="status" className="text-text-muted">
      Đang mở hồ sơ…
    </p>
  );
  if (customer.isError)
    return (
      <p role="alert" className="field-error">
        {messageFor(customer.error)}
      </p>
    );
  if (!customer.data) return loading;
  return (
    <section aria-label="Thêm thông tin khách" className="grid gap-2">
      <h3 className="section-label">Thêm thông tin khách (không bắt buộc)</h3>
      <Suspense fallback={loading}>
        <CustomerForm
          customer={customer.data}
          canWriteHealth={canWriteHealth}
          onSaved={onSaved}
          onCancel={onCancel}
        />
      </Suspense>
    </section>
  );
}

/**
 * The bars above the cart lines: "Đang ghi cho lịch hẹn …" (⋯ → "Bỏ liên kết lịch hẹn") when the
 * visit completes a booking, and "Đã khôi phục giỏ chưa lưu" (+ "Bỏ giỏ") after a draft came back.
 */
export function CartContextBars({
  correcting,
  onStopCorrecting,
  appointment,
  today,
  restoredFor,
  onUnlink,
  onDiscard,
}: {
  /** "Sửa lượt": the recorded visit the cart corrects ("Bỏ sửa" leaves it). */
  correcting: Visit | null;
  onStopCorrecting: () => void;
  appointment: Appointment | null;
  today: string;
  /** The customer whose unsaved cart was restored, or null. */
  restoredFor: string | null;
  onUnlink: () => void;
  onDiscard: () => void;
}) {
  return (
    <>
      {correcting && (
        <div className="context-bar">
          <span>
            Đang sửa lượt {timeOf(correcting.check_in_at)} ·{" "}
            {dmy(correcting.business_date)} của {correcting.customer_name}
          </span>
          <button
            type="button"
            className="btn btn-ghost min-h-9 px-3"
            onClick={onStopCorrecting}
          >
            Bỏ sửa
          </button>
        </div>
      )}
      {appointment && (
        <div className="context-bar">
          <span>
            Đang ghi cho lịch hẹn {timeOf(appointment.starts_at)}
            {appointment.business_date !== today
              ? ` · ${dmy(appointment.business_date)}`
              : ""}{" "}
            · {appointment.customer_name}
          </span>
          <details
            className="context-menu"
            onKeyDown={(e) => {
              const d = e.currentTarget;
              if (e.key === "Escape" && d.open) {
                e.stopPropagation();
                d.open = false;
                d.querySelector("summary")?.focus();
              }
            }}
          >
            <summary aria-label="Tùy chọn lịch hẹn">
              <Icon name="dots" />
            </summary>
            <div className="context-menu-panel">
              <button type="button" onClick={onUnlink}>
                Bỏ liên kết lịch hẹn
              </button>
              <p>
                Lịch hẹn vẫn mở (chưa hoàn tất). Lượt này được ghi riêng, như
                khách đến không hẹn.
              </p>
            </div>
          </details>
        </div>
      )}
      {restoredFor !== null && (
        <div role="status" className="context-bar">
          <span>Đã khôi phục giỏ chưa lưu của {restoredFor}.</span>
          <button
            type="button"
            className="btn btn-ghost min-h-9 px-3"
            onClick={onDiscard}
          >
            Bỏ giỏ
          </button>
        </div>
      )}
    </>
  );
}

/**
 * "Đổi khách" while the cart holds items asks first (EM live test: the cart vanished on a click).
 * A new visit's cart stays as that customer's draft; in "Sửa lượt" the lines go (the visit moves
 * to the new customer only when saved).
 */
export function LeaveCartConfirm({
  customerName,
  correcting,
  onLeave,
  onStay,
}: {
  customerName: string;
  correcting: boolean;
  onLeave: () => void;
  onStay: () => void;
}) {
  const leave = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    leave.current?.focus();
  }, []);
  return (
    <div
      role="alertdialog"
      aria-labelledby="leave-cart-title"
      aria-describedby="leave-cart-body"
      className="callout"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          onStay();
        }
      }}
    >
      <p id="leave-cart-title" className="font-semibold">
        Đổi khách? Giỏ của {customerName} chưa lưu.
      </p>
      <p id="leave-cart-body">
        {correcting
          ? "Dịch vụ trong giỏ sẽ bỏ, chọn lại cho khách mới. Lượt đã ghi chỉ đổi khi bấm “Lưu sửa lượt”."
          : `Giỏ được giữ lại: chọn lại ${customerName} để ghi tiếp.`}
      </p>
      <div className="button-row">
        <button
          ref={leave}
          type="button"
          className="btn btn-primary"
          onClick={onLeave}
        >
          Đổi khách
        </button>
        <button type="button" className="btn btn-secondary" onClick={onStay}>
          Ở lại
        </button>
      </div>
    </div>
  );
}

/** ISO day `days` before `day`. */
function daysBefore(day: string, days: number): string {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() - days);
  return d.toISOString().slice(0, 10);
}

const dmy = (iso: string) => iso.split("-").reverse().join("/");

/** "Giờ làm": now by default; the desk may pick an earlier time today, the owner an earlier day. */
export function ServiceTime({
  mayBackdate,
  today,
  day,
  time,
  invalid,
  onDay,
  onTime,
}: {
  mayBackdate: boolean;
  today: string;
  day: string;
  time: string;
  invalid: boolean;
  onDay: (iso: string) => void;
  onTime: (hm: string) => void;
}) {
  const [open, setOpen] = useState(day !== "" || time !== "");
  const backdated = day !== "" && day < today;
  const value =
    time === "" && !backdated
      ? "Bây giờ"
      : `${time || "Chưa chọn giờ"}${backdated ? ` · ${dmy(day)}` : " hôm nay"}`;
  return (
    <div className="option-row">
      <span className="option-label">Giờ làm</span>
      <span className="option-value">{value}</span>
      {open ? (
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => {
            onDay(today);
            onTime("");
            setOpen(false);
          }}
        >
          Dùng giờ hiện tại
        </button>
      ) : (
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => {
            setOpen(true);
          }}
        >
          {mayBackdate ? "Đổi giờ / nhập bù" : "Đổi giờ làm"}
        </button>
      )}
      {open && (
        <fieldset className="option-panel">
          <legend className="sr-only">Giờ làm dịch vụ</legend>
          <div className="option-fields">
            {mayBackdate && (
              <DateField
                label="Ngày làm"
                value={day || today}
                min={daysBefore(today, 366)}
                max={today}
                onChange={(iso) => {
                  onDay(iso || today);
                }}
              />
            )}
            <label className="field">
              <span>Giờ (để trống = bây giờ)</span>
              <TimeField
                value={time}
                invalid={invalid}
                onChange={onTime}
                className="input w-40"
              />
            </label>
          </div>
          {backdated && (
            <p className="text-sm text-warning-text">
              Nhập bù: lượt, hóa đơn và tiền thu được ghi vào ngày {dmy(day)}.
            </p>
          )}
        </fieldset>
      )}
    </div>
  );
}

/** Discount on the one-off lines: % or amount + reason (D10). The server computes and checks. */
export function DiscountField({
  due,
  draft,
  overCap,
  onChange,
}: {
  due: number;
  draft: DiscountDraft | null;
  overCap: boolean;
  onChange: (d: DiscountDraft | null) => void;
}) {
  const amount = draft ? discountPreview(due, draft) : 0;
  return (
    <div className="option-row">
      <span className="option-label">Giảm giá</span>
      <span className="option-value" aria-live="polite">
        {!draft
          ? "Không"
          : amount > 0
            ? `Giảm ${formatVnd(amount)}`
            : "Nhập mức giảm hợp lệ."}
      </span>
      {draft ? (
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => {
            onChange(null);
          }}
        >
          Bỏ giảm giá
        </button>
      ) : (
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => {
            onChange({ kind: "percent", value: "", reason: "" });
          }}
        >
          Thêm giảm giá
        </button>
      )}
      {draft && (
        <fieldset className="option-panel">
          <legend className="sr-only">Giảm giá dịch vụ lẻ</legend>
          <div className="option-fields">
            <div role="group" aria-label="Kiểu giảm" className="segmented">
              {(
                [
                  ["percent", "%"],
                  ["amount", "Số tiền"],
                ] as const
              ).map(([k, label]) => (
                <button
                  key={k}
                  type="button"
                  aria-pressed={draft.kind === k}
                  onClick={() => {
                    onChange({ ...draft, kind: k, value: "" });
                  }}
                >
                  {label}
                </button>
              ))}
            </div>
            <label className="field">
              <span>
                {draft.kind === "percent" ? "Mức giảm (%)" : "Số tiền giảm (đ)"}
              </span>
              <input
                inputMode="numeric"
                value={draft.value}
                onChange={(e) => {
                  onChange({ ...draft, value: e.target.value });
                }}
                className="input w-36"
              />
            </label>
          </div>
          <label className="field">
            <span>Lý do (bắt buộc)</span>
            <input
              value={draft.reason}
              maxLength={500}
              onChange={(e) => {
                onChange({ ...draft, reason: e.target.value });
              }}
              className="input"
            />
          </label>
          {overCap && (
            <p className="field-error">
              Vượt mức quản lý được giảm: cần chủ spa.
            </p>
          )}
        </fieldset>
      )}
    </div>
  );
}

/**
 * One-off service: grouped by category, ★ first; price by the customer's gender. Picking a
 * service adds it (no separate "Thêm": a pick that waited for it was lost, booking bug #44). A
 * closed select also changes value on ↑/↓ and type-ahead, so a keyboard pick waits for Enter.
 */
export function AddOneOff({
  customer,
  onAdd,
  onProblem,
  id = "one-off-service",
  label = "Thêm dịch vụ lẻ",
  lines = [],
}: {
  customer: CustomerSummary;
  /**
   * The cart already holding lines priced Nữ/Nam: the toggle shows that choice until the desk
   * picks again (a restored draft or "Sửa lượt" brought the Nam price back but not the toggle).
   */
  lines?: CartLine[];
  onAdd: (
    service: Service,
    priceVariant: Gender | null,
    price: number,
    categoryName: string | undefined,
  ) => void;
  onProblem: (message: string) => void;
  /** Field id (unique on the page) and visible label. */
  id?: string;
  label?: string;
}) {
  const services = useQuery(servicesQuery(false));
  const categories = useQuery(categoriesQuery);
  const [pending, setPending] = useState("");
  const viaKeys = useRef(false);
  const [chosenGender, setChosenGender] = useState<Gender | null>(null);
  const known = customer.gender ?? null;
  const fromCart =
    lines.flatMap((l) =>
      l.source === "one_off" && l.priceVariant ? [l.priceVariant] : [],
    )[0] ?? null;
  const gender = known ?? chosenGender ?? fromCart;

  const groups = (categories.data ?? []).map((c) => ({
    category: c,
    items: (services.data ?? [])
      .filter((s) => s.category_id === c.id)
      .sort((a, b) => Number(b.recommended) - Number(a.recommended)),
  }));

  const add = (serviceId: string) => {
    setPending("");
    const service = services.data?.find((s) => s.id === serviceId);
    if (!service) return;
    const r = priceFor(service, gender);
    if (!r.ok) {
      onProblem(
        r.reason === "need_gender"
          ? "Chọn Nữ hoặc Nam để tính giá dịch vụ này."
          : r.reason === "not_offered"
            ? `“${service.name}” không áp dụng cho khách ${gender === "male" ? "Nam" : "Nữ"}.`
            : `“${service.name}” chưa có giá.`,
      );
      return;
    }
    onAdd(
      service,
      known ? null : r.variant === "all" ? null : r.variant,
      r.price,
      categories.data?.find((c) => c.id === service.category_id)?.name,
    );
  };

  const priceLabel = (s: Service) =>
    s.prices.all != null
      ? formatVnd(s.prices.all)
      : `Nữ ${s.prices.female != null ? formatVnd(s.prices.female) : "—"} / Nam ${s.prices.male != null ? formatVnd(s.prices.male) : "—"}`;
  const pendingName = services.data?.find((s) => s.id === pending)?.name;

  return (
    <div className="grid gap-2">
      <label htmlFor={id} className="section-label">
        {label}
      </label>
      {services.isError && (
        <p role="alert" className="field-error">
          {messageFor(services.error)}
        </p>
      )}
      <div className="flex flex-wrap items-center gap-2">
        {!known && (
          <span
            role="group"
            aria-label="Giới tính để tính giá"
            className="segmented"
          >
            {(
              [
                ["female", "Nữ"],
                ["male", "Nam"],
              ] as const
            ).map(([g, label]) => (
              <button
                key={g}
                type="button"
                aria-pressed={gender === g}
                onClick={() => {
                  setChosenGender(g);
                }}
              >
                {label}
              </button>
            ))}
          </span>
        )}
        <select
          id={id}
          value={pending}
          aria-describedby={pending ? `${id}-pending` : undefined}
          onPointerDown={() => {
            viaKeys.current = false;
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              if (pending) {
                e.preventDefault();
                add(pending);
              }
              return;
            }
            // Keys that step through a closed select; Space / Alt+↓ / F4 open the list instead.
            viaKeys.current =
              (!e.altKey &&
                /^(ArrowUp|ArrowDown|Home|End|PageUp|PageDown)$/.test(e.key)) ||
              (e.key.length === 1 && e.key !== " ");
          }}
          onChange={(e) => {
            const id = e.target.value;
            if (viaKeys.current) setPending(id);
            else add(id);
          }}
          // A definite width: a select's intrinsic width is its longest option, which would
          // otherwise widen the whole column past the viewport.
          className="input w-56 min-w-0 flex-1"
        >
          <option value="">
            {services.isPending
              ? "Đang tải bảng giá…"
              : "Chọn dịch vụ để thêm…"}
          </option>
          {groups
            .filter((g) => g.items.length > 0)
            .map((g) => (
              <optgroup key={g.category.id} label={g.category.name}>
                {g.items.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.recommended ? "★ " : ""}
                    {s.name} · {priceLabel(s)}
                  </option>
                ))}
              </optgroup>
            ))}
        </select>
      </div>
      {pendingName ? (
        <p id={`${id}-pending`} className="field-hint">
          Nhấn Enter để thêm “{pendingName}”.
        </p>
      ) : (
        !known && (
          <p className="field-hint">
            Khách chưa có giới tính: chọn Nữ/Nam để tính giá.
          </p>
        )
      )}
    </div>
  );
}

/** "Người giới thiệu" options: khách tự chọn, then every active staff member with their role. */
export function ReferrerOptions({ staff }: { staff: Staff[] }) {
  return (
    <>
      <option value="">Khách tự chọn</option>
      {staff.map((s) => (
        <option key={s.id} value={s.id}>
          {s.full_name} ({s.position})
        </option>
      ))}
    </>
  );
}

/** "Giới thiệu" on a one-off line: who recommended it ("" = khách tự chọn, Q-U2). */
export function LineReferrer({
  id,
  serviceName,
  value,
  staff,
  onChange,
}: {
  id: string;
  serviceName: string;
  value: string;
  staff: Staff[];
  onChange: (referrerId: string) => void;
}) {
  return (
    <>
      <label htmlFor={id}>Giới thiệu</label>
      <select
        id={id}
        aria-label={`Người giới thiệu ${serviceName}`}
        value={value}
        onChange={(e) => {
          onChange(e.target.value);
        }}
        className="input"
      >
        <ReferrerOptions staff={staff} />
      </select>
    </>
  );
}

/**
 * The cart's service lines: name, where the session comes from (package, a package sold today,
 * one-off) and its price, ✕, the technician (focus lands here after "Trừ 1 buổi", fast path step
 * 3) and, for a one-off, who recommended it. Lives in this lazy chunk (prefetched after the first
 * paint) so the startup bundle stays under budget.
 */
export function CartLines({
  lines,
  onLines,
  covered,
  planName,
  attempted,
  staff,
  staffPending,
  technicians,
  staffSelects,
  focusLineKey,
  openKey,
  onOpen,
}: {
  /**
   * The one line open for editing (EM 2026-11-07: a finished item folds into a summary row with
   * Sửa / ✕). A line without its technician stays open; `undefined` = every line open.
   */
  openKey?: string | null;
  /** `"l<key>"` of the line to open, or null to fold every finished one. */
  onOpen?: (item: string | null) => void;
  lines: CartLine[];
  onLines: (update: (ls: CartLine[]) => CartLine[]) => void;
  /** Line key → `ref` of the package sold today that covers it. */
  covered: ReadonlyMap<number, string>;
  planName: (ref: string) => string;
  attempted: boolean;
  /** Everyone working here (the referrer may be any of them). */
  staff: Staff[];
  staffPending: boolean;
  /** Technicians, busiest today first. */
  technicians: Staff[];
  /** The page focuses a missing technician picker on save. */
  staffSelects: RefObject<Map<number, HTMLSelectElement>>;
  /** Line whose technician picker takes focus when it mounts. */
  focusLineKey: RefObject<number | null>;
}) {
  const staffName = (id: string) =>
    staff.find((s) => s.id === id)?.full_name ??
    technicians.find((s) => s.id === id)?.full_name;
  return (
    <ul aria-label="Dịch vụ trong lượt" className="cart-lines">
      {lines.map((l) => {
        const invalid = attempted && l.staffId === "";
        const selectId = `staff-${String(l.key)}`;
        const coveredBy = covered.get(l.key);
        const name = serviceLabel(l.serviceName, l.categoryName);
        const sourceText =
          l.source === "package"
            ? `Trừ liệu trình · ${l.packageName}${l.expiryOverrideReason ? " · cho dùng tiếp" : ""}`
            : l.source === "new_package"
              ? `Buổi hôm nay của gói mua hôm nay · ${planName(l.saleRef)}`
              : coveredBy
                ? `Trừ gói mua hôm nay · ${planName(coveredBy)}`
                : "Dịch vụ lẻ";
        const price = (
          <span className="cart-line-price">
            {l.source === "one_off" ? (
              coveredBy ? (
                <>
                  <s className="cart-line-was">{formatVnd(l.price)}</s>
                  {formatVnd(0)}
                </>
              ) : (
                formatVnd(l.price)
              )
            ) : l.source === "new_package" ? (
              formatVnd(0)
            ) : null}
          </span>
        );
        const remove = (
          <button
            type="button"
            aria-label={`Bỏ dòng ${l.serviceName}`}
            onClick={() => {
              onLines((ls) => ls.filter((x) => x.key !== l.key));
            }}
            className="btn btn-ghost btn-icon text-text-muted"
          >
            <Icon name="x" />
          </button>
        );
        const folded =
          onOpen !== undefined && openKey !== `l${String(l.key)}` && l.staffId !== "";
        if (folded) {
          const referrer =
            l.source === "one_off" && !coveredBy
              ? ` · Giới thiệu: ${l.referrerId ? (staffName(l.referrerId) ?? "…") : "Khách tự chọn"}`
              : "";
          return (
            <li key={l.key} className="cart-line is-summary">
              <div className="min-w-0">
                <span className="cart-line-name">{name}</span>
                <span className="cart-line-source">
                  KTV {staffName(l.staffId) ?? "…"} · {sourceText}
                  {referrer}
                </span>
              </div>
              {price}
              <button
                type="button"
                aria-label={`Sửa dòng ${l.serviceName}`}
                className="btn btn-ghost"
                onClick={() => {
                  onOpen(`l${String(l.key)}`);
                }}
              >
                Sửa
              </button>
              {remove}
            </li>
          );
        }
        return (
          <li key={l.key} className="cart-line">
            <div className="min-w-0">
              <span className="cart-line-name">{name}</span>
              <span className="cart-line-source">{sourceText}</span>
            </div>
            {price}
            {remove}
            <div className="cart-line-staff">
              <label htmlFor={selectId}>KTV</label>
              <select
                id={selectId}
                ref={(el) => {
                  if (!el) {
                    staffSelects.current.delete(l.key);
                    return;
                  }
                  staffSelects.current.set(l.key, el);
                  if (focusLineKey.current === l.key) {
                    focusLineKey.current = null;
                    el.focus();
                  }
                }}
                value={l.staffId}
                aria-invalid={invalid}
                aria-required="true"
                aria-describedby={invalid ? `${selectId}-error` : undefined}
                onChange={(e) => {
                  const staffId = e.target.value;
                  onLines((ls) =>
                    ls.map((x) => (x.key === l.key ? { ...x, staffId } : x)),
                  );
                }}
                className="input"
              >
                <option value="">
                  {staffPending ? "Đang tải…" : "Chọn KTV…"}
                </option>
                {technicians.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.full_name}
                  </option>
                ))}
              </select>
              {invalid && (
                <span id={`${selectId}-error`} className="field-error">
                  Cần chọn KTV
                </span>
              )}
              {l.source === "one_off" && !coveredBy && (
                <LineReferrer
                  id={`ref-${String(l.key)}`}
                  serviceName={l.serviceName}
                  value={l.referrerId ?? ""}
                  staff={staff}
                  onChange={(referrerId) => {
                    onLines((ls) =>
                      ls.map((x) =>
                        x.key === l.key && x.source === "one_off"
                          ? { ...x, referrerId }
                          : x,
                      ),
                    );
                  }}
                />
              )}
              {onOpen !== undefined && l.staffId !== "" && (
                <button
                  type="button"
                  aria-label={`Xong dòng ${l.serviceName}`}
                  className="btn btn-ghost cart-line-done"
                  onClick={() => {
                    onOpen(null);
                  }}
                >
                  Xong
                </button>
              )}
            </div>
          </li>
        );
      })}
    </ul>
  );
}

/** "Thanh toán (thu đủ)": three radio chips, and the cash part when paid both ways. */
export function PaymentChoices({
  name,
  mode,
  onMode,
  cashText,
  onCashText,
  total,
  invalid,
}: {
  /** Radio group name, unique on the page. */
  name: string;
  mode: PaymentMode;
  onMode: (mode: PaymentMode) => void;
  cashText: string;
  onCashText: (text: string) => void;
  /** What must be collected (preview). */
  total: number;
  invalid: boolean;
}) {
  const cash = parseAmount(cashText);
  const typed = cashText.trim() !== "";
  // Said as soon as it is typed, not only after "Lưu": a red border alone did not say why.
  const problem =
    typed && Number.isNaN(cash)
      ? "Tiền mặt: chỉ nhập chữ số."
      : typed && cash >= total
        ? `Tiền mặt phải nhỏ hơn số phải trả (${formatVnd(total)}). Khách trả hết bằng tiền mặt thì chọn “Tiền mặt”.`
        : invalid
          ? "Nhập phần khách trả bằng tiền mặt (lớn hơn 0)."
          : null;
  return (
    <fieldset className="grid gap-2">
      <legend className="section-label mb-2">Thanh toán (thu đủ)</legend>
      <div className="pay-choices">
        {(
          [
            ["cash", "Tiền mặt"],
            ["bank_transfer", "Chuyển khoản"],
            ["mixed", "Tiền mặt + CK"],
          ] as const
        ).map(([value, label]) => (
          <label key={value} className="pay-choice">
            <input
              type="radio"
              name={name}
              checked={mode === value}
              onChange={() => {
                onMode(value);
              }}
            />
            {label}
          </label>
        ))}
      </div>
      {mode === "mixed" && (
        // The message sits outside the label: inside, it became part of the field's name.
        <div className="field max-w-xs">
          <label htmlFor={`${name}-cash`}>Tiền mặt (đ)</label>
          <input
            id={`${name}-cash`}
            className="input"
            inputMode="numeric"
            value={cashText}
            aria-invalid={invalid || problem !== null}
            aria-describedby={`${name}-cash-msg`}
            onChange={(e) => {
              onCashText(e.target.value);
            }}
          />
          {problem ? (
            <span id={`${name}-cash-msg`} role="alert" className="field-error">
              {problem}
            </span>
          ) : (
            <span id={`${name}-cash-msg`} className="field-hint">
              {cash > 0 && cash < total
                ? `Chuyển khoản: ${formatVnd(total - cash)}`
                : `Phần còn lại của ${formatVnd(total)} là chuyển khoản.`}
            </span>
          )}
        </div>
      )}
    </fieldset>
  );
}
