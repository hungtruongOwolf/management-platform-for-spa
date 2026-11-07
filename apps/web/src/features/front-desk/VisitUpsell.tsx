// Round 6 on a saved visit (spec R1-1 §10.4.3–10.4.4), opened in place under its row of "Lượt
// khách hôm nay":
// One entry, "Thêm dịch vụ / bán gói" (EM 2026-11-07: no second "Chuyển sang gói" action):
// - new lines and packages → `POST /visits/{id}/additions`: the recorded ones never change, no
//   reason (a sale, not a fix);
// - exactly one package, nothing else, whose plan covers a one-off service done today → the desk
//   may tick "Tính buổi hôm nay vào gói" (Q-U1, case by case, default off) →
//   `POST /visits/{id}/convert-to-package`: today's one-off becomes session 1. The money already
//   paid keeps its method (the server records it again); the desk only takes the top-up.
// A lazy chunk. Amounts are previews; the server prices everything.
import type { Visit } from "@vpyd/api-client";
import {
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import { Suspense, useCallback, useRef, useState } from "react";
import { Icon } from "../../app/Icon";
import { ApiError } from "../../lib/api";
import { messageFor } from "../../lib/errors";
import { invalidateManagementViews } from "../../lib/invalidate";
import { serviceLabel } from "../../lib/names";
import { can, meQuery } from "../auth/session";
import { plansQuery } from "../catalog/api";
import { activeStaffQuery, parseAmount } from "../sales/api";
import {
  type CartLine,
  type PackageSaleDraft,
  type PaymentMode,
  amountDue,
  coverage,
  packagesDue,
  paymentsFor,
  savedSummary,
  visitBody,
} from "./api";
import {
  LineReferrer,
  PackageSales,
  packageSalesBody,
  saleProblem,
} from "./CartPackages";
import {
  addToVisit,
  canCountToday,
  conversionMoney,
  coveredLines,
} from "./upsell";
import { AddOneOff, PaymentChoices } from "./CartOptions";

/** "TM" / "CK" / "TM + CK" of the visit's one-off payments. */
function methodsOf(v: Visit): string {
  const m = new Set(
    v.payments.filter((p) => p.kind === "payment").map((p) => p.method),
  );
  return [m.has("cash") ? "TM" : "", m.has("bank_transfer") ? "CK" : ""]
    .filter(Boolean)
    .join(" + ");
}

/** What the server's refusal means at the desk (412, 409 and the conversion's own codes). */
export function upsellMessage(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.status === 412)
      return "Lượt vừa được sửa ở máy khác. Đóng lại rồi mở lượt mới nhất.";
    if (e.code === "visit.nothing_to_convert")
      return "Gói này không có dịch vụ lẻ nào của lượt: bỏ chọn “Tính buổi hôm nay vào gói”.";
  }
  return messageFor(e);
}

export function VisitUpsell({
  visit,
  mayConvert,
  onCancel,
  onDone,
}: {
  visit: Visit;
  /** The caller may also convert (`package.sell` + `record.void_same_day`; UX only). */
  mayConvert: boolean;
  onCancel: () => void;
  /** The visit after the change (the corrected one after a conversion) + the toast text. */
  onDone: (visit: Visit, summary: string) => void;
}) {
  const { data: me } = useSuspenseQuery(meQuery);
  const queryClient = useQueryClient();
  const staff = useQuery(activeStaffQuery);
  const plans = useQuery(plansQuery(false));
  const [idempotencyKey] = useState(() => crypto.randomUUID());
  const firstStaff = visit.lines[0]?.staff_id ?? "";
  const customer = {
    id: visit.customer_id,
    full_name: visit.customer_name,
    phone: null,
    named: true,
    gender: visit.customer_gender ?? null,
  };
  const context = visit.lines.map((l) => ({
    serviceId: l.service_id,
    staffId: l.staff_id,
  }));
  const [lines, setLines] = useState<CartLine[]>([]);
  const [sales, setSales] = useState<PackageSaleDraft[]>([]);
  const [open, setOpen] = useState<string | null>(null);
  const clearOpen = useCallback(() => {
    setOpen(null);
  }, []);
  const [countToday, setCountToday] = useState(false);
  const [payMode, setPayMode] = useState<PaymentMode>("cash");
  const [cashText, setCashText] = useState("");
  const [attempted, setAttempted] = useState(false);
  const nextKey = useRef(1);

  const covered = coverage(lines, sales);
  const sale = sales[0];
  const plan = sale?.plan
    ? plans.data?.find((p) => p.id === sale.plan?.id)
    : undefined;
  const mayCount =
    mayConvert && lines.length === 0 && sales.length === 1 && plan && !sale?.forOther
      ? canCountToday(visit, plan)
      : false;
  const converting = mayCount && countToday;
  const pkgTotal = packagesDue(sales);
  // A conversion costs the package + the one-off lines kept. What was paid today counts towards it
  // in its own method (cash stays cash on the day's books); the payment choice is only for the rest.
  const conversion =
    converting && plan && sale
      ? conversionMoney(visit, plan, plan.price - pkgTotal)
      : null;
  const due = conversion
    ? Math.max(0, conversion.handOver)
    : amountDue(lines, covered) + pkgTotal;
  const cashPart = parseAmount(cashText);
  const nothing = lines.length === 0 && sales.length === 0;
  const problems = {
    staff: lines.some((l) => l.staffId === ""),
    cash:
      due > 0 &&
      payMode === "mixed" &&
      (Number.isNaN(cashPart) || cashPart <= 0 || cashPart >= due),
    sale: sales
      .map((s) => saleProblem(s, can(me, "sale.discount_unlimited")))
      .find((p) => p !== null),
  };

  const mutation = useMutation({
    mutationFn: async (): Promise<{ visit: Visit; summary: string }> => {
      const payments = paymentsFor(due, payMode, cashPart);
      if (converting && sale) {
        const [body] = packageSalesBody([sale]);
        if (!body) throw new Error("no plan");
        const m = await import("./upsell");
        const done = await m.convertToPackage(
          visit,
          { package_sale: body, payments },
          idempotencyKey,
        );
        return {
          visit: done.visit,
          summary: `Đã chuyển lượt của ${visit.customer_name} sang ${sale.plan?.name ?? "gói"} · ${savedSummary(done.visit).split(" · ").slice(1).join(" · ")}`,
        };
      }
      const { lines: bodyLines, package_sales } = visitBody(
        visit.customer_id,
        lines,
        payments,
        "",
        { packageSales: packageSalesBody(sales), covered },
      );
      const saved = await addToVisit(
        visit.id,
        {
          lines: bodyLines,
          ...(package_sales ? { package_sales } : {}),
          payments,
        },
        idempotencyKey,
      );
      const sold = saved.package_sales
        .slice(visit.package_sales.length)
        .map((p) => p.description);
      return {
        visit: saved,
        summary: [
          `Đã thêm vào lượt của ${visit.customer_name}`,
          ...lines.map((l) => l.serviceName),
          ...sold.map((d) => `bán ${d}`),
        ].join(" · "),
      };
    },
    onSuccess: ({ visit: saved, summary }) => {
      void queryClient.invalidateQueries({ queryKey: ["visits"] });
      void queryClient.invalidateQueries({ queryKey: ["customer-packages"] });
      void queryClient.invalidateQueries({ queryKey: ["appointments"] });
      void invalidateManagementViews(queryClient, visit.customer_id);
      onDone(saved, summary);
    },
  });

  const problem = nothing
    ? "Chưa có gì để thêm: chọn dịch vụ hoặc bán gói."
    : (problems.sale ??
      (problems.staff
        ? "Mỗi dịch vụ cần chọn KTV thực hiện."
        : problems.cash
          ? "Tiền mặt phải lớn hơn 0 và nhỏ hơn số phải thu."
          : null));

  const submit = () => {
    setAttempted(true);
    if (problem || mutation.isPending) return;
    mutation.mutate();
  };

  const covers =
    plan && converting
      ? coveredLines(visit, plan)
          .map((l) => serviceLabel(l.service_name, l.category_name))
          .join(", ")
      : "";
  const title = "Thêm dịch vụ / bán gói";

  return (
    <form
      noValidate
      aria-label={`${title} · lượt của ${visit.customer_name}`}
      className="visit-inline-form"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <p className="panel-title">{title}</p>
      {!converting && (
        <>
          {lines.length > 0 && (
            <ul aria-label="Dịch vụ thêm vào lượt" className="cart-lines">
              {lines.map((l) => {
                const coveredBy = covered.get(l.key);
                return (
                  <li key={l.key} className="cart-line">
                    <div className="min-w-0">
                      <span className="cart-line-name">
                        {serviceLabel(l.serviceName, l.categoryName)}
                      </span>
                      <span className="cart-line-source">
                        {coveredBy ? "Trừ gói mua hôm nay" : "Dịch vụ lẻ"}
                      </span>
                    </div>
                    <span className="cart-line-price">
                      {l.source === "one_off" &&
                        formatVnd(coveredBy ? 0 : l.price)}
                    </span>
                    <button
                      type="button"
                      aria-label={`Bỏ dòng ${l.serviceName}`}
                      className="btn btn-ghost btn-icon text-text-muted"
                      onClick={() => {
                        setLines((ls) => ls.filter((x) => x.key !== l.key));
                      }}
                    >
                      <Icon name="x" />
                    </button>
                    <div className="cart-line-staff">
                      <label htmlFor={`up-staff-${String(l.key)}`}>KTV</label>
                      <select
                        id={`up-staff-${String(l.key)}`}
                        className="input"
                        value={l.staffId}
                        aria-invalid={attempted && l.staffId === ""}
                        onChange={(e) => {
                          const staffId = e.target.value;
                          setLines((ls) =>
                            ls.map((x) =>
                              x.key === l.key ? { ...x, staffId } : x,
                            ),
                          );
                        }}
                      >
                        <option value="">Chọn KTV…</option>
                        {(staff.data ?? []).map((s) => (
                          <option key={s.id} value={s.id}>
                            {s.full_name}
                          </option>
                        ))}
                      </select>
                      {l.source === "one_off" && !coveredBy && (
                        <LineReferrer
                          id={`up-ref-${String(l.key)}`}
                          serviceName={l.serviceName}
                          value={l.referrerId ?? ""}
                          staff={staff.data ?? []}
                          onChange={(referrerId) => {
                            setLines((ls) =>
                              ls.map((x) =>
                                x.key === l.key && x.source === "one_off"
                                  ? { ...x, referrerId }
                                  : x,
                              ),
                            );
                          }}
                        />
                      )}
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
          <Suspense fallback={<div aria-hidden="true" className="h-[78px]" />}>
            <AddOneOff
              id={`add-one-off-${visit.id}`}
              label="Thêm dịch vụ vào lượt"
              customer={customer}
              onAdd={(service, priceVariant, price, categoryName) => {
                setLines((ls) => [
                  ...ls,
                  {
                    key: nextKey.current++,
                    source: "one_off",
                    serviceId: service.id,
                    serviceName: service.name,
                    ...(categoryName ? { categoryName } : {}),
                    priceVariant,
                    price,
                    staffId: firstStaff,
                    // Added after the visit was saved: the technician sold it (upsell).
                    referrerId: firstStaff,
                  },
                ]);
              }}
              onProblem={() => undefined}
            />
          </Suspense>
        </>
      )}
      {can(me, "package.sell") && (
        <PackageSales
          sales={sales}
          onSales={setSales}
          lines={lines}
          covered={covered}
          staff={staff.data ?? []}
          customerId={visit.customer_id}
          canDiscount={
            can(me, "sale.discount") || can(me, "sale.discount_unlimited")
          }
          unlimitedDiscount={can(me, "sale.discount_unlimited")}
          attempted={attempted}
          serverError={null}
          openPlanId={open}
          onOpened={clearOpen}
          context={context}
          maxSales={converting ? 1 : 3}
          titleId={`upsell-packages-${visit.id}`}
          title="Bán gói cho lượt này"
        />
      )}
      {mayCount && (
        <label className="check-row">
          <input
            type="checkbox"
            checked={countToday}
            onChange={(e) => {
              setCountToday(e.target.checked);
            }}
          />
          <span>
            Tính buổi hôm nay vào gói
            <span className="check-hint">
              {countToday && conversion
                ? `Hôm nay khách đã trả ${formatVnd(visit.total)}${methodsOf(visit) ? ` (${methodsOf(visit)})` : ""} cho ${covers}: số tiền này tính vào gói, giữ nguyên cách trả; buổi hôm nay là buổi 1 của gói.`
                : "Mặc định không: lượt lẻ giữ nguyên, chỉ bán gói mới."}
            </span>
          </span>
        </label>
      )}
      {!nothing && due > 0 && (
        <PaymentChoices
          name={`upsell-payment-${visit.id}`}
          mode={payMode}
          onMode={setPayMode}
          cashText={cashText}
          onCashText={setCashText}
          total={due}
          invalid={attempted && problems.cash}
        />
      )}
      <div className="upsell-total" aria-live="polite">
        <span>
          {conversion
            ? conversion.handOver < 0
              ? "Trả lại khách"
              : "Khách đưa thêm"
            : "Thu thêm"}
        </span>
        <span className="checkout-amount">
          {formatVnd(conversion ? Math.abs(conversion.handOver) : due)}
        </span>
      </div>
      {conversion && (
        <p className="checkout-split">
          Gói {formatVnd(pkgTotal)} − đã trả hôm nay {formatVnd(visit.total)}
          {conversion.due - pkgTotal > 0
            ? ` + dịch vụ lẻ giữ lại ${formatVnd(conversion.due - pkgTotal)}`
            : ""}
        </p>
      )}
      {attempted && problem && (
        <p role="alert" className="field-error">
          {problem}
        </p>
      )}
      {mutation.isError && (
        <p role="alert" className="alert">
          {upsellMessage(mutation.error)}
        </p>
      )}
      <div className="button-row">
        <button
          type="submit"
          aria-disabled={mutation.isPending}
          aria-busy={mutation.isPending}
          className="btn btn-primary"
        >
          {mutation.isPending
            ? "Đang lưu…"
            : converting
              ? "Chuyển sang gói"
              : "Lưu phần thêm"}
        </button>
        <button type="button" className="btn btn-secondary" onClick={onCancel}>
          Thôi
        </button>
      </div>
    </form>
  );
}
