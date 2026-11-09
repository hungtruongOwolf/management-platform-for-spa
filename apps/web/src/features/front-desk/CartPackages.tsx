// Round 6 (spec R1-1 §10.4.1): the cart's "Gói bán hôm nay" — packages sold in the same checkout,
// each with its own "người giới thiệu / chốt", gift, discount and "Dùng gói này cho buổi hôm nay"
// — and the unsaved-cart draft. A lazy chunk (prefetched after the first paint): the startup
// bundle only carries the draft type and the totals. Every amount here is a preview; the server
// prices each package from the catalog.
import type { CustomerSummary, PackagePlan, Staff } from "@vpyd/api-client";
import { useQuery } from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import { useEffect, useId, useRef, useState } from "react";
import { Icon } from "../../app/Icon";
import { ApiError } from "../../lib/api";
import { messageFor } from "../../lib/errors";
import { serviceLabel } from "../../lib/names";
import { categoriesQuery, plansQuery, servicesQuery } from "../catalog/api";
import { CustomerSearch } from "../customers/CustomerSearch";
import { exceedsCap } from "../sales/api";
import { type CartLine, type PackageSaleDraft, packageDiscount } from "./api";
import { SELF_CHOSEN } from "./checkout";
import {
  type CartDraft,
  clearDraft,
  latestDraftCustomer,
  loadDraft,
  restoreCart,
  saveDraft,
} from "./draftCart";
import { ReferrerOptions } from "./CartOptions";
import { defaultCloser } from "./upsell";
import "./cart-packages.css";

export { SELF_CHOSEN, packageSalesBody, sellCart, sellPackageOnly } from "./checkout";
export { LineReferrer, ReferrerOptions } from "./CartOptions";

/** Why a nameless walk-in cannot own the package (server: `422 customer.name_required`). */
export const OWNER_NAME_NEEDED =
  "Gói liệu trình cần chủ gói có tên: bấm “Thêm thông tin khách” để ghi tên, hoặc chọn “Mua cho người khác”.";

/**
 * Sessions of packages sold today that can no longer stand: their package left the cart, became
 * "mua cho người khác" (not shared, D5), or no longer holds that service. Same array when none.
 */
export function pruneSessions(
  lines: CartLine[],
  sales: PackageSaleDraft[],
): CartLine[] {
  const keep = lines.filter((l) => {
    if (l.source !== "new_package") return true;
    const s = sales.find((x) => x.ref === l.saleRef);
    return (
      s !== undefined &&
      !s.forOther &&
      !s.beneficiary &&
      s.plan !== null &&
      s.plan.items.some((i) => i.service_id === l.serviceId)
    );
  });
  return keep.length === lines.length ? lines : keep;
}

/** At most this many packages per checkout (server: 0–3). */
export const MAX_PACKAGE_SALES = 3;

/** "10 buổi + tặng 2 · HSD 6 tháng". */
export function sessionsText(
  plan: Pick<PackagePlan, "items" | "validity_months">,
): string {
  const paid = plan.items.reduce((n, i) => n + i.sessions, 0);
  const bonus = plan.items.reduce((n, i) => n + i.bonus_sessions, 0);
  const validity = plan.validity_months
    ? ` · HSD ${String(plan.validity_months)} tháng`
    : "";
  return `${String(paid)} buổi${bonus ? ` + tặng ${String(bonus)}` : ""}${validity}`;
}

/** A new, empty package block; `ref` is unique in the cart. */
export function newSale(
  sales: PackageSaleDraft[],
  plan: PackageSaleDraft["plan"] = null,
): PackageSaleDraft {
  const used = new Set(sales.map((s) => s.ref));
  let n = 1;
  while (used.has(`p${String(n)}`)) n++;
  return {
    ref: `p${String(n)}`,
    plan,
    closer: "",
    giftNote: "",
    discountText: "",
    discountReason: "",
    useToday: false,
    beneficiary: null,
  };
}

/** Does the plan cover a one-off line of the cart (so "Dùng gói này cho buổi hôm nay" means something)? */
export function coversCart(
  plan: Pick<PackagePlan, "items"> | null,
  lines: CartLine[],
): boolean {
  if (!plan) return false;
  const services = new Set(plan.items.map((i) => i.service_id));
  return lines.some((l) => l.source === "one_off" && services.has(l.serviceId));
}

/** Why this package cannot be saved yet (shown at the block), or null. */
export function saleProblem(
  p: PackageSaleDraft,
  unlimitedDiscount: boolean,
  ownerNamed = true,
): string | null {
  if (!p.plan) return "Chọn mẫu gói liệu trình.";
  // ADR-0011: a package belongs to a named person; a nameless walk-in may still pay for one.
  if (!ownerNamed && !p.forOther) return OWNER_NAME_NEEDED;
  if (p.closer === "") return "Chọn người giới thiệu (hoặc Khách tự chọn).";
  if (p.forOther && !p.beneficiary) return "Chọn khách sẽ dùng gói.";
  const d = packageDiscount(p);
  if (Number.isNaN(d)) return "Giảm giá gói: chỉ nhập chữ số.";
  if (d > p.plan.price) return "Giảm giá lớn hơn giá gói.";
  if (d > 0 && p.discountReason.trim() === "")
    return "Giảm giá gói phải ghi lý do.";
  if (d > 0 && exceedsCap(p.plan.price, d, unlimitedDiscount))
    return "Vượt 10 % giá gói, cần chủ spa.";
  return null;
}

/** The first reason the cart's packages cannot be saved yet, or null (checked before sending). */
export function checkoutProblem(
  sales: PackageSaleDraft[],
  lineCount: number,
  forAppointment: boolean,
  unlimitedDiscount: boolean,
  ownerNamed = true,
): string | null {
  for (const s of sales) {
    const p = saleProblem(s, unlimitedDiscount, ownerNamed);
    if (p) return p;
  }
  if (lineCount === 0 && forAppointment)
    return "Lịch hẹn cần ít nhất một dịch vụ trong lượt. Thêm dịch vụ, hoặc bỏ liên kết lịch hẹn để chỉ bán gói.";
  return null;
}

/** A refusal that concerns the packages of the cart (shown at their blocks), else null. */
export function saleErrorOf(e: unknown): string | null {
  return e instanceof ApiError &&
    (e.errors.some((f) => f.field.startsWith("package_sales")) ||
      e.code === "discount.limit_exceeded")
    ? messageFor(e)
    : null;
}

/** Plans that cover a service of the cart first (spec: "ưu tiên gói chứa dịch vụ lẻ"), then by name. */
export function plansFor(
  plans: PackagePlan[],
  serviceIds: string[],
): PackagePlan[] {
  const wanted = new Set(serviceIds);
  const hits = (p: PackagePlan) =>
    p.items.some((i) => wanted.has(i.service_id)) ? 0 : 1;
  return [...plans].sort(
    (a, b) => hits(a) - hits(b) || a.name.localeCompare(b.name, "vi"),
  );
}

/**
 * "Ghi chú gói" (F4): collapsed behind one ghost button, so a package costs no extra step unless
 * the desk wants a note. Health data like the session note: only holders of
 * `customer.health_notes.write` see the field, the server refuses a note from anyone else.
 */
function PackageNoteField({
  note,
  onChange,
}: {
  note: string;
  onChange: (note: string) => void;
}) {
  const id = useId();
  const [open, setOpen] = useState(note !== "");
  if (!open)
    return (
      <button
        type="button"
        className="btn btn-ghost"
        onClick={() => {
          setOpen(true);
        }}
      >
        Thêm ghi chú gói
      </button>
    );
  return (
    <div className="field">
      <label htmlFor={`${id}-note`}>
        Ghi chú gói{" "}
        <span className="font-normal text-text-muted">(không bắt buộc)</span>
      </label>
      <textarea
        id={`${id}-note`}
        className="input"
        rows={2}
        maxLength={2000}
        autoFocus={note === ""}
        aria-describedby={`${id}-hint`}
        value={note}
        onChange={(e) => {
          onChange(e.target.value);
        }}
      />
      <span id={`${id}-hint`} className="field-hint">
        Bảo mật như ghi chú buổi: chỉ người được xem lưu ý sức khỏe mới đọc
        được. Thông tin sức khỏe chỉ ghi ở đây, không ghi vào Quà tặng hay lý
        do giảm giá.
      </span>
      <button
        type="button"
        className="btn btn-ghost justify-self-start"
        onClick={() => {
          setOpen(false);
          onChange("");
        }}
      >
        Bỏ ghi chú gói
      </button>
    </div>
  );
}

/** One package block: plan, closer, gift, discount, "use today", "buy for someone else", note. */
function SaleBlock({
  sale,
  index,
  lines,
  context,
  covered,
  plans,
  serviceNames,
  staff,
  customerId,
  canDiscount,
  unlimitedDiscount,
  attempted,
  serverError,
  onChange,
  onRemove,
  ownerNamed,
  folded,
  onEdit,
  onDone,
  onTodaySession,
  canWriteNote,
}: {
  /** `customer.health_notes.write`: the "Ghi chú gói" field is offered (UX only). */
  canWriteNote: boolean;
  /** The buyer has a name (a nameless walk-in may only buy for someone else, ADR-0011). */
  ownerNamed: boolean;
  /** Finished and not the item being edited: one summary row with Sửa / ✕. */
  folded: boolean;
  onEdit?: () => void;
  onDone?: () => void;
  /** "Làm 1 buổi hôm nay": today's session of this service from this package. */
  onTodaySession?: (serviceId: string) => void;
  sale: PackageSaleDraft;
  index: number;
  lines: CartLine[];
  context: { serviceId: string; staffId: string }[];
  covered: ReadonlyMap<number, string>;
  plans: PackagePlan[];
  serviceNames: Map<string, string>;
  staff: Staff[];
  customerId: string;
  canDiscount: boolean;
  unlimitedDiscount: boolean;
  attempted: boolean;
  serverError: string | null;
  onChange: (patch: Partial<PackageSaleDraft>) => void;
  onRemove: () => void;
}) {
  const id = useId();
  const forOther = sale.forOther === true;
  const [discounting, setDiscounting] = useState(sale.discountText !== "");
  const plan = sale.plan;
  const full = plans.find((p) => p.id === plan?.id);
  const discount = packageDiscount(sale);
  const problem = saleProblem(sale, unlimitedDiscount, ownerNamed);
  const usesToday = [...covered.values()].filter((r) => r === sale.ref).length;
  const canUseToday =
    coversCart(plan, lines) && !sale.beneficiary && !forOther && ownerNamed;
  const coveredNames = plan
    ? plan.items
        .map((i) => serviceNames.get(i.service_id))
        .filter((n): n is string => n !== undefined)
    : [];
  const inactive = plan !== null && !full;
  // Today's sessions from this package: "Làm 1 buổi hôm nay" lines and covered one-off lines.
  const todayIds = new Set(
    lines
      .filter(
        (l) =>
          (l.source === "new_package" && l.saleRef === sale.ref) ||
          covered.get(l.key) === sale.ref,
      )
      .map((l) => l.serviceId),
  );
  const freeItems =
    plan && onTodaySession && ownerNamed && !forOther && !sale.beneficiary
      ? plan.items.filter((i) => !todayIds.has(i.service_id))
      : [];
  const todayNames = [...todayIds].map(
    (s) => serviceNames.get(s) ?? "dịch vụ",
  );
  const shownPrice = plan
    ? plan.price - (Number.isNaN(discount) ? 0 : Math.min(discount, plan.price))
    : 0;
  const price = (
    <span className="sale-price">
      {plan && discount > 0 && !Number.isNaN(discount) && (
        <s>{formatVnd(plan.price)}</s>
      )}
      {plan ? formatVnd(shownPrice) : ""}
    </span>
  );
  const remove = (
    <button
      type="button"
      aria-label={`Bỏ ${plan ? plan.name : "gói này"}`}
      className="btn btn-ghost btn-icon text-text-muted"
      onClick={onRemove}
    >
      <Icon name="x" />
    </button>
  );

  if (folded && plan) {
    const closer =
      sale.closer === SELF_CHOSEN || sale.closer === ""
        ? "Khách tự chọn"
        : (staff.find((s) => s.id === sale.closer)?.full_name ?? "…");
    const meta = [
      `Người giới thiệu: ${closer}`,
      sale.beneficiary ? `Mua cho ${sale.beneficiary.full_name}` : "",
      discount > 0 && !Number.isNaN(discount)
        ? `Giảm ${formatVnd(discount)}`
        : "",
      todayIds.size > 0 ? `Hôm nay làm ${todayNames.join(", ")}` : "",
      canWriteNote && sale.note?.trim() ? "Có ghi chú gói" : "",
    ]
      .filter(Boolean)
      .join(" · ");
    return (
      <li className="sale-block is-summary" aria-labelledby={`${id}-title`}>
        <div className="sale-head is-summary">
          <span className="min-w-0">
            <span id={`${id}-title`} className="sale-title">
              {plan.name}
            </span>
            <span className="cart-line-source">{meta}</span>
          </span>
          {price}
          <button
            type="button"
            aria-label={`Sửa gói ${plan.name}`}
            className="btn btn-ghost"
            onClick={onEdit}
          >
            Sửa
          </button>
          {remove}
        </div>
      </li>
    );
  }

  return (
    <li className="sale-block" aria-labelledby={`${id}-title`}>
      <div className="sale-head">
        <span id={`${id}-title`} className="sale-title">
          {plan ? plan.name : `Gói thứ ${String(index + 1)}`}
        </span>
        {price}
        {remove}
      </div>
      {freeItems.length > 0 && (
        <div className="sale-today">
          {freeItems.map((i) => (
            <button
              key={i.service_id}
              type="button"
              className="btn btn-secondary"
              aria-label={
                plan && plan.items.length > 1
                  ? `Làm 1 buổi hôm nay: ${serviceNames.get(i.service_id) ?? "dịch vụ"}`
                  : "Làm 1 buổi hôm nay"
              }
              onClick={() => {
                onTodaySession?.(i.service_id);
              }}
            >
              <Icon name="plus" />
              {plan && plan.items.length > 1
                ? `Làm 1 buổi hôm nay: ${serviceNames.get(i.service_id) ?? "dịch vụ"}`
                : "Làm 1 buổi hôm nay"}
            </button>
          ))}
          <span className="check-hint">
            Khách mua gói và làm luôn hôm nay: buổi này trừ vào gói vừa mua
            (0đ), lưu cùng một lần.
          </span>
        </div>
      )}
      {todayIds.size > 0 && (
        <p className="field-hint">
          Hôm nay làm {todayNames.join(", ")} từ gói này (0đ). KTV chọn ở
          dòng dịch vụ phía trên.
        </p>
      )}
      <div className="sale-fields">
        <label className="field sale-wide">
          <span>Mẫu gói</span>
          <select
            className="input"
            value={plan?.id ?? ""}
            aria-invalid={attempted && !plan}
            autoFocus={!plan}
            onChange={(e) => {
              const p = plans.find((x) => x.id === e.target.value) ?? null;
              const next = p
                ? {
                    id: p.id,
                    name: p.name,
                    price: p.price,
                    items: p.items,
                  }
                : null;
              const closer = defaultCloser(context, next);
              onChange({
                plan: next,
                giftNote: p?.gift_note ?? "",
                // The technician serving the customer by default (Q-U2), until the desk picks.
                ...(sale.closer === "" || sale.closer === SELF_CHOSEN
                  ? { closer: closer ?? SELF_CHOSEN }
                  : {}),
                useToday: !sale.beneficiary && coversCart(next, lines),
              });
            }}
          >
            <option value="">Chọn gói…</option>
            {inactive && (
              <option value={plan.id}>{plan.name} (đã ngừng bán)</option>
            )}
            {plans.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name} · {formatVnd(p.price)} ({sessionsText(p)})
              </option>
            ))}
          </select>
          {inactive && (
            <span className="field-error">
              Mẫu gói đã ngừng bán, chọn gói khác.
            </span>
          )}
        </label>
        <label className="field">
          <span>Người giới thiệu / chốt</span>
          <select
            className="input"
            value={sale.closer === SELF_CHOSEN ? "" : sale.closer}
            onChange={(e) => {
              onChange({ closer: e.target.value || SELF_CHOSEN });
            }}
          >
            <ReferrerOptions staff={staff} />
          </select>
        </label>
        <label className="field">
          <span>Quà tặng</span>
          <input
            className="input"
            value={sale.giftNote}
            maxLength={500}
            onChange={(e) => {
              onChange({ giftNote: e.target.value });
            }}
          />
        </label>
      </div>
      {canUseToday && (
        <label className="check-row">
          <input
            type="checkbox"
            checked={sale.useToday}
            onChange={(e) => {
              onChange({ useToday: e.target.checked });
            }}
          />
          <span>
            Dùng gói này cho buổi hôm nay
            <span className="check-hint">
              {sale.useToday && usesToday > 0
                ? `Trừ ${String(usesToday)} buổi ${coveredNames.join(", ")} từ gói này, dòng lẻ về 0đ`
                : `Dòng lẻ ${coveredNames.join(", ")} sẽ trừ buổi từ gói này`}
            </span>
          </span>
        </label>
      )}
      <label className="check-row">
        <input
          type="checkbox"
          checked={forOther}
          onChange={(e) => {
            onChange(
              e.target.checked
                ? { forOther: true, useToday: false }
                : {
                    forOther: false,
                    beneficiary: null,
                    useToday: coversCart(plan, lines),
                  },
            );
          }}
        />
        <span>
          Mua cho người khác
          <span className="check-hint">
            Gói nằm trên hồ sơ người dùng; khách này trả tiền.
          </span>
        </span>
      </label>
      {forOther &&
        (sale.beneficiary ? (
          <div className="option-row">
            <span className="option-label">Người dùng gói</span>
            <span className="option-value">{sale.beneficiary.full_name}</span>
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => {
                onChange({ beneficiary: null });
              }}
            >
              Đổi người
            </button>
          </div>
        ) : (
          <CustomerSearch
            onSelect={(c: CustomerSummary) => {
              if (c.id === customerId) return;
              onChange({
                beneficiary: { id: c.id, full_name: c.full_name },
                useToday: false,
              });
            }}
            onCreate={() => undefined}
          />
        ))}
      {canDiscount &&
        (discounting ? (
          <div className="sale-fields">
            <label className="field">
              <span>Giảm giá gói (đ)</span>
              <input
                className="input"
                inputMode="numeric"
                value={sale.discountText}
                aria-invalid={attempted && Number.isNaN(discount)}
                onChange={(e) => {
                  onChange({ discountText: e.target.value });
                }}
              />
            </label>
            <label className="field">
              <span>Lý do giảm giá</span>
              <input
                className="input"
                value={sale.discountReason}
                maxLength={500}
                onChange={(e) => {
                  onChange({ discountReason: e.target.value });
                }}
              />
            </label>
            <button
              type="button"
              className="btn btn-ghost sale-wide"
              onClick={() => {
                setDiscounting(false);
                onChange({ discountText: "", discountReason: "" });
              }}
            >
              Bỏ giảm giá gói
            </button>
          </div>
        ) : (
          <button
            type="button"
            className="btn btn-ghost"
            onClick={() => {
              setDiscounting(true);
            }}
          >
            Thêm giảm giá gói
          </button>
        ))}
      {canWriteNote && (
        <PackageNoteField
          note={sale.note ?? ""}
          onChange={(note) => {
            onChange({ note });
          }}
        />
      )}
      {(attempted && problem) || serverError ? (
        <p role="alert" className="field-error">
          {serverError ?? problem}
        </p>
      ) : (
        problem === OWNER_NAME_NEEDED && (
          <p className="field-hint">{problem}</p>
        )
      )}
      {onDone && plan && !problem && (
        <button type="button" className="btn btn-ghost" onClick={onDone}>
          Xong
        </button>
      )}
    </li>
  );
}

/**
 * "Gói bán hôm nay": the packages of this checkout and the "+ Bán gói liệu trình" action. A cart
 * without services holds up to three packages too (EM 2026-11-07 item 4): one save, one payment.
 */
export function PackageSales({
  sales,
  onSales,
  lines,
  covered,
  staff,
  customerId,
  canDiscount,
  unlimitedDiscount,
  attempted,
  serverError,
  openPlanId,
  onOpened,
  context = lines,
  maxSales = MAX_PACKAGE_SALES,
  titleId = "cart-packages-title",
  title = "Gói bán hôm nay",
  ownerNamed = true,
  openRef,
  onOpenRef,
  onTodaySession,
  onLines,
  canWriteNote = false,
}: {
  /**
   * `customer.health_notes.write` (UX only): each package offers an optional, collapsed
   * "Ghi chú gói" (F4). Off by default.
   */
  canWriteNote?: boolean;
  /** Lets the block drop today's sessions of a package that left the cart (`pruneSessions`). */
  onLines?: (update: (ls: CartLine[]) => CartLine[]) => void;
  /** The buyer has a name (else a package must be bought for someone else, ADR-0011). */
  ownerNamed?: boolean;
  /**
   * The block open for editing (one item open at a time in the /quay cart); a finished block
   * folds into a summary row. Absent = every block open (other screens).
   */
  openRef?: string | null;
  /** `"s<ref>"` of the block to open, or null to fold every finished one. */
  onOpenRef?: (item: string | null) => void;
  /** "Làm 1 buổi hôm nay" on a block: add today's session of that service from it. */
  onTodaySession?: (
    ref: string,
    service: { serviceId: string; serviceName: string; categoryName?: string },
  ) => void;
  sales: PackageSaleDraft[];
  onSales: (update: (s: PackageSaleDraft[]) => PackageSaleDraft[]) => void;
  lines: CartLine[];
  /**
   * What the visit holds (services + technicians): the default closer and the plans offered
   * first. Defaults to the cart lines; a saved visit passes its own lines.
   */
  context?: { serviceId: string; staffId: string }[];
  /** How many packages fit (3; converting a visit into a package fits one). */
  maxSales?: number;
  titleId?: string;
  title?: string;
  covered: ReadonlyMap<number, string>;
  staff: Staff[];
  customerId: string;
  canDiscount: boolean;
  unlimitedDiscount: boolean;
  attempted: boolean;
  /** The server's refusal of a package (shown at the blocks, the cart is kept). */
  serverError: unknown;
  /** "Mua lại gói này" / "Bán gói liệu trình" from the customer card: open a block (prefilled). */
  openPlanId: string | null;
  onOpened: () => void;
}) {
  const plans = useQuery(plansQuery(false));
  const services = useQuery(servicesQuery(false));
  const categories = useQuery(categoriesQuery);
  const serviceNames = new Map(
    (services.data ?? []).map((s) => [
      s.id,
      serviceLabel(
        s.name,
        categories.data?.find((c) => c.id === s.category_id)?.name,
      ),
    ]),
  );
  const ordered = plansFor(
    plans.data ?? [],
    context.map((l) => l.serviceId),
  );
  const full = sales.length >= maxSales;

  // Opened from the customer card: a new block, prefilled with that plan when it still sells.
  const openedFrom = { context, lines, sales, open: onOpenRef };
  const latest = useRef(openedFrom);
  useEffect(() => {
    latest.current = openedFrom;
  });
  // One block per request: StrictMode (and a refetch of the plans) runs this effect again before
  // `onOpened` has cleared the request, which opened "Gói thứ 2" empty (EM live test).
  const handled = useRef(false);
  useEffect(() => {
    if (openPlanId === null) {
      handled.current = false;
      return;
    }
    if (!plans.isSuccess || handled.current) return;
    handled.current = true;
    onOpened();
    if (full) return;
    const p = plans.data.find((x) => x.id === openPlanId);
    const plan = p
      ? { id: p.id, name: p.name, price: p.price, items: p.items }
      : null;
    const { context: ctx, lines: ls, sales: current, open } = latest.current;
    const added = {
      ...newSale(current, plan),
      ...(plan
        ? {
            closer: defaultCloser(ctx, plan) ?? SELF_CHOSEN,
            giftNote: p?.gift_note ?? "",
            useToday: coversCart(plan, ls),
          }
        : {}),
    };
    onSales((s) => (s.length >= maxSales ? s : [...s, added]));
    open?.(`s${added.ref}`);
  }, [openPlanId, plans.isSuccess, plans.data, full, maxSales, onOpened, onSales]);

  useEffect(() => {
    onLines?.((ls) => pruneSessions(ls, sales));
  }, [sales, onLines]);

  const update = (ref: string, patch: Partial<PackageSaleDraft>) => {
    onSales((s) => s.map((x) => (x.ref === ref ? { ...x, ...patch } : x)));
  };

  return (
    <section aria-labelledby={titleId} className="grid gap-2">
      <h3 id={titleId} className="section-label">
        {title}
      </h3>
      {plans.isError && (
        <p role="alert" className="field-error">
          {messageFor(plans.error)}
        </p>
      )}
      {sales.length > 0 && (
        <ul className="sale-list" aria-label="Gói bán hôm nay">
          {sales.map((s, i) => (
            <SaleBlock
              key={s.ref}
              sale={s}
              index={i}
              lines={lines}
              context={context}
              covered={covered}
              plans={ordered}
              serviceNames={serviceNames}
              staff={staff}
              customerId={customerId}
              canDiscount={canDiscount}
              unlimitedDiscount={unlimitedDiscount}
              attempted={attempted}
              serverError={saleErrorOf(serverError)}
              ownerNamed={ownerNamed}
              canWriteNote={canWriteNote}
              folded={
                onOpenRef !== undefined &&
                openRef !== `s${s.ref}` &&
                !saleErrorOf(serverError) &&
                saleProblem(s, unlimitedDiscount, ownerNamed) === null
              }
              {...(onOpenRef
                ? {
                    onEdit: () => {
                      onOpenRef(`s${s.ref}`);
                    },
                    onDone: () => {
                      onOpenRef(null);
                    },
                  }
                : {})}
              {...(onTodaySession
                ? {
                    onTodaySession: (serviceId: string) => {
                      const sv = services.data?.find((x) => x.id === serviceId);
                      const category = categories.data?.find(
                        (c) => c.id === sv?.category_id,
                      )?.name;
                      onTodaySession(s.ref, {
                        serviceId,
                        serviceName: sv?.name ?? "Dịch vụ của gói",
                        ...(category ? { categoryName: category } : {}),
                      });
                    },
                  }
                : {})}
              onChange={(patch) => {
                update(s.ref, patch);
              }}
              onRemove={() => {
                onSales((x) => x.filter((y) => y.ref !== s.ref));
              }}
            />
          ))}
        </ul>
      )}
      {!full && (
        <button
          type="button"
          className="btn btn-secondary"
          onClick={() => {
            const added = newSale(sales);
            onSales((s) => [...s, added]);
            onOpenRef?.(`s${added.ref}`);
          }}
        >
          <Icon name="plus" />
          Bán gói liệu trình
        </button>
      )}
    </section>
  );
}

/**
 * Keeps the unsaved cart in the tab's draft (spec §10.8 Q-U8): saves on every change, clears an
 * emptied cart, brings a customer's draft back when they are picked again, and after a reload
 * reopens this user's newest draft. Renders nothing; a lazy piece so the startup bundle does not
 * carry it.
 */
export function DraftKeeper({
  userId,
  draft,
  customerId,
  fresh,
  onRestore,
}: {
  userId: string;
  /** The cart now; null while no customer is picked or a recorded visit is being corrected. */
  draft: CartDraft | null;
  customerId: string | null;
  /** The customer was just picked for a fresh start (a booking, a correction): no restore. */
  fresh: boolean;
  onRestore: (d: CartDraft) => void;
}) {
  const seen = useRef<string | null | undefined>(undefined);
  const restore = useRef(onRestore);
  useEffect(() => {
    restore.current = onRestore;
  });
  // A new customer (or the first mount): bring back what was left unsaved.
  useEffect(() => {
    const first = seen.current === undefined;
    if (seen.current === customerId) return;
    seen.current = customerId;
    const id = customerId ?? (first ? latestDraftCustomer(userId) : null);
    if (!id || fresh) return;
    const d = restoreCart(loadDraft(userId, id));
    if (d) restore.current(d);
  }, [customerId, fresh, userId]);
  // The page builds `draft` on every render: write only when it really changed.
  const last = useRef("");
  useEffect(() => {
    if (!draft) return;
    const text = JSON.stringify(draft);
    if (text === last.current) return;
    last.current = text;
    const id = draft.customer.id;
    if (draft.lines.length === 0 && draft.sales.length === 0)
      clearDraft(userId, id);
    else saveDraft(userId, id, draft);
  });
  return null;
}
