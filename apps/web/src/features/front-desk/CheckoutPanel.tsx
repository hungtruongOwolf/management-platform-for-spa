import type {
  Appointment,
  CustomerSummary,
  Staff,
  Visit,
  VisitCreate,
  VisitPackageSaleCreate,
} from "@vpyd/api-client";
import { useQueryClient, type UseMutationResult } from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import {
  Suspense,
  lazy,
  useEffect,
  type Dispatch,
  type RefObject,
  type SetStateAction,
} from "react";
import { ApiError } from "../../lib/api";
import { fieldMessage, messageFor } from "../../lib/errors";
import { invalidateManagementViews } from "../../lib/invalidate";
import { can, type Me } from "../auth/session";
import { completeAppointment } from "../appointments/complete";
import { exceedsCap, parseAmount } from "../sales/api";
import {
  type CartLine,
  type DiscountDraft,
  type PackageSaleDraft,
  type PaymentMode,
  amountDue,
  byServiceTime,
  coverage,
  discountPreview,
  packagesDue,
  paymentsFor,
  recordVisit,
  savedSummary,
  serviceInstant,
  staffByUse,
  timeOf,
  todayVisitsQuery,
  visitBody,
} from "./api";
import type {
  CorrectionDraft,
  CorrectionPlan,
  CorrectionSaved,
  CorrectionStart,
} from "./correction";

// The cart side of /quay ("Lượt khách đang ghi": lines, payment, note, Lưu) and what each save does.
// Its own chunk, fetched right after the first paint with the cart's other chunks: the first screen
// shows only the empty cart, which FrontDeskPage draws until this lands, so none of this code is on
// /quay's critical path (frontend.md "Perf budget"). The cart's state and the saves' state stay in
// FrontDeskPage, which the customer side and the keyboard shortcuts share.

const loadCartOptions = () => import("./CartOptions");
const ServiceTime = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.ServiceTime })),
);
const DiscountField = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.DiscountField })),
);
const AddOneOff = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.AddOneOff })),
);
const CartContextBars = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.CartContextBars })),
);
const CartLines = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.CartLines })),
);
const PaymentChoices = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.PaymentChoices })),
);
// Round 6: packages sold in the checkout (spec §10.4.1). Prefetched after the first paint.
const loadCartPackages = () => import("./CartPackages");
const PackageSales = lazy(() =>
  loadCartPackages().then((m) => ({ default: m.PackageSales })),
);
// "Sửa lượt" (US6b): only when a recorded visit is opened for correction.
const loadCorrection = () => import("./correction");
const loadCorrectionPanel = () => import("./CorrectionPanel");
const CorrectionPanel = lazy(() =>
  loadCorrectionPanel().then((m) => ({ default: m.CorrectionPanel })),
);
const CorrectionTotal = lazy(() =>
  loadCorrectionPanel().then((m) => ({ default: m.CorrectionTotal })),
);

// The one-off picker's box (label + field) while its chunk loads: nothing shifts when it lands.
const addOneOffBox = <div aria-hidden="true" className="h-[78px]" />;
// "Gói bán hôm nay" (label + one button) while its chunk loads.
const packagesBox = <div aria-hidden="true" className="h-[70px]" />;

/** "HH:MM" now in the spa's zone. */
function nowHm(): string {
  return timeOf(new Date().toISOString());
}

/** A correction worth sending (the "none" plan never reaches the server). */
export type CorrectionToSave = Exclude<CorrectionPlan, { kind: "none" }>;

/**
 * What each save of the desk does (FrontDeskPage's mutations call these): set by the checkout,
 * where every save starts.
 */
export interface DeskSaves {
  /** "Lưu" (the button, Ctrl+Enter). */
  save: () => void;
  recordVisit: (body: VisitCreate) => Promise<Visit>;
  visitSaved: (visit: Visit, body: VisitCreate) => void;
  visitFailed: (error: Error) => Promise<void>;
  saveCorrection: (plan: CorrectionToSave) => Promise<CorrectionSaved>;
  correctionSaved: (saved: CorrectionSaved) => void;
  correctionFailed: (error: Error) => void;
  sellPackages: (drafts: PackageSaleDraft[]) => Promise<string>;
  packagesSold: (soldText: string, drafts: PackageSaleDraft[]) => void;
  packagesFailed: (error: Error) => void;
}

export interface CheckoutPanelProps {
  me: Me | null;
  customer: CustomerSummary | null;
  lines: CartLine[];
  onLines: Dispatch<SetStateAction<CartLine[]>>;
  sales: PackageSaleDraft[];
  onSales: Dispatch<SetStateAction<PackageSaleDraft[]>>;
  mode: PaymentMode;
  onMode: (mode: PaymentMode) => void;
  cashText: string;
  onCashText: (text: string) => void;
  note: string;
  onNote: (note: string) => void;
  discount: DiscountDraft | null;
  onDiscount: (discount: DiscountDraft | null) => void;
  serviceDay: string;
  onServiceDay: (day: string) => void;
  serviceTime: string;
  onServiceTime: (time: string) => void;
  correcting: CorrectionStart | null;
  reason: string;
  onReason: (reason: string) => void;
  appointment: Appointment | null;
  onAppointment: (appointment: Appointment | null) => void;
  companions: string[];
  restored: boolean;
  attempted: boolean;
  onAttempted: (attempted: boolean) => void;
  openItem: string | null;
  onOpenItem: (item: string | null) => void;
  openPlan: string | null;
  onOpenedPlan: () => void;
  onFocusDeduct: (focus: boolean) => void;
  staff: Staff[];
  staffPending: boolean;
  today: Visit[];
  idempotencyKey: string;
  todayIso: string;
  staffSelectsRef: RefObject<Map<number, HTMLSelectElement>>;
  focusLineKeyRef: RefObject<number | null>;
  /** The next cart line's key. */
  nextKeyRef: RefObject<number>;
  addLine: (line: CartLine) => void;
  resetVisit: () => void;
  select: (c: CustomerSummary, fresh?: boolean) => void;
  forgetDraft: (customerId: string | undefined) => void;
  showToast: (text: string, error?: boolean) => void;
  visit: UseMutationResult<Visit, Error, VisitCreate>;
  correction: UseMutationResult<CorrectionSaved, Error, CorrectionToSave>;
  packageOnly: UseMutationResult<string, Error, PackageSaleDraft[]>;
  /** Where the checkout leaves its `DeskSaves` for FrontDeskPage. */
  savesRef: RefObject<DeskSaves | null>;
}

/** Everything of the cart under its title, from "Chọn khách…" to "Lưu lượt khách". */
export function CheckoutPanel({
  me,
  customer,
  lines,
  onLines: setLines,
  sales,
  onSales: setSales,
  mode,
  onMode: setMode,
  cashText,
  onCashText: setCashText,
  note,
  onNote: setNote,
  discount,
  onDiscount: setDiscount,
  serviceDay,
  onServiceDay: setServiceDay,
  serviceTime,
  onServiceTime: setServiceTime,
  correcting,
  reason,
  onReason: setReason,
  appointment,
  onAppointment: setAppointment,
  companions,
  restored,
  attempted,
  onAttempted: setAttempted,
  openItem,
  onOpenItem: setOpenItem,
  openPlan,
  onOpenedPlan: clearOpenPlan,
  onFocusDeduct: setFocusDeduct,
  staff,
  staffPending,
  today,
  idempotencyKey,
  todayIso,
  staffSelectsRef: staffSelects,
  focusLineKeyRef: focusLineKey,
  nextKeyRef: nextKey,
  addLine,
  resetVisit,
  select,
  forgetDraft,
  showToast,
  visit: mutation,
  correction,
  packageOnly,
  savesRef,
}: CheckoutPanelProps) {
  const queryClient = useQueryClient();

  // Lines a package sold today covers are today's session from it: 0đ (spec §10.4.1).
  const covered = coverage(lines, sales);
  const due = amountDue(lines, covered);
  const discountAmount = discountPreview(due, discount);
  const packagesTotal = packagesDue(sales);
  const payable = due - discountAmount + packagesTotal;
  const planOf = (ref: string | undefined) =>
    sales.find((s) => s.ref === ref)?.plan?.name ?? "gói mua hôm nay";
  const cashPart = parseAmount(cashText);
  const technicians = staffByUse(staff, today);
  const mayBackdate = can(me, "visit.backdate");

  const saving =
    mutation.isPending || correction.isPending || packageOnly.isPending;
  const correctionDraft: CorrectionDraft | null =
    correcting && customer
      ? {
          customerId: customer.id,
          lines,
          payments: paymentsFor(payable, mode, cashPart),
          discount: due > 0 ? discount : null,
          checkInAt: serviceInstant(serviceDay || todayIso, serviceTime),
          note,
          canWriteNote: can(me, "customer.health_notes.write"),
          reason,
        }
      : null;

  // No one-off amount left (a package covers the lines): the field is gone and nothing is sent.
  const discountProblem =
    due > 0 &&
    discount !== null &&
    (discountAmount <= 0 || discount.reason.trim() === "");
  const problems = {
    staff: lines.some((l) => l.staffId === ""),
    cash:
      payable > 0 &&
      mode === "mixed" &&
      (Number.isNaN(cashPart) || cashPart <= 0 || cashPart >= payable),
    time: serviceDay !== "" && serviceTime === "",
  };

  /** `packageSales`: the cart's packages in wire form (built by the lazy cart chunk). */
  const record = (packageSales?: VisitPackageSaleCreate[]) => {
    if (!customer) return;
    mutation.mutate(
      visitBody(
        customer.id,
        lines,
        paymentsFor(payable, mode, cashPart),
        can(me, "customer.health_notes.write") ? note : "",
        {
          discount: due > 0 ? discount : null,
          checkInAt: serviceInstant(serviceDay || todayIso, serviceTime),
          ...(packageSales ? { packageSales, covered } : {}),
          companions,
        },
      ),
    );
  };

  const save = () => {
    if (!customer || (lines.length === 0 && sales.length === 0) || saving)
      return;
    setAttempted(true);
    if (problems.staff) {
      showToast("Mỗi dịch vụ cần chọn KTV thực hiện.", true);
      const missing = lines.find((l) => l.staffId === "");
      if (missing) staffSelects.current.get(missing.key)?.focus();
      return;
    }
    if (problems.cash) {
      showToast("Tiền mặt phải lớn hơn 0 và nhỏ hơn số phải trả.", true);
      return;
    }
    if (discountProblem) {
      showToast("Giảm giá cần mức giảm hợp lệ và lý do.", true);
      return;
    }
    if (problems.time) {
      showToast("Chọn giờ làm cho ngày nhập bù.", true);
      return;
    }
    if (correcting && correctionDraft) {
      const old = correcting;
      void loadCorrection().then((m) => {
        const plan = m.planCorrection(old.visit, old.note, correctionDraft);
        if (plan.kind === "none") {
          showToast("Chưa có gì thay đổi so với lượt đã ghi.", true);
          return;
        }
        if (plan.kind === "correct" && plan.body.reason === "") {
          showToast("Cần ghi lý do sửa lượt.", true);
          return;
        }
        correction.mutate(plan);
      });
      return;
    }
    if (sales.length > 0) {
      void loadCartPackages().then((m) => {
        const problem = m.checkoutProblem(
          sales,
          lines.length,
          appointment !== null,
          can(me, "sale.discount_unlimited"),
          customer.named,
        );
        if (problem) showToast(problem, true);
        else if (lines.length > 0) record(m.packageSalesBody(sales));
        else packageOnly.mutate(sales);
      });
      return;
    }
    record();
  };

  // What the desk's three saves do, with this render's cart (FrontDeskPage holds their state).
  const saves: DeskSaves = {
    save,
    recordVisit: async (body) => {
      if (!appointment) return recordVisit(body, idempotencyKey);
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { customer_id, ...visit } = body;
      const done = await completeAppointment(
        appointment.id,
        visit,
        idempotencyKey,
      );
      return done.visit;
    },
    visitSaved: (visit, body) => {
      const sentNote = body.note;
      // Both cached lists of today: the peek and, once "Xem tất cả" opened it, the whole day.
      queryClient.setQueriesData<Visit[]>(
        { queryKey: todayVisitsQuery.queryKey },
        (old) =>
          old && visit.business_date === todayIso
            ? byServiceTime([
                // The saved response never carries the note (health data): show the one just typed.
                { ...visit, ...(sentNote ? { note: sentNote } : {}) },
                ...old.filter((v) => v.id !== visit.id),
              ])
            : old,
      );
      // Walk-ins are on the calendar too (spec appointments.md §10.1), not only completed bookings.
      void queryClient.invalidateQueries({ queryKey: ["appointments"] });
      if (visit.business_date !== todayIso)
        void queryClient.invalidateQueries({
          queryKey: ["visits", visit.business_date],
        });
      // Not awaited: the toast does not wait a round trip for the balance (perf page §4d). The
      // balance refetches behind it and has a 5 s staleTime, so the next open of this customer asks again.
      void queryClient.invalidateQueries({
        queryKey: ["customer-packages", visit.customer_id],
      });
      void invalidateManagementViews(queryClient, visit.customer_id);
      // The server wrote the Nữ / Nam picked for pricing on a profile without a gender (F2).
      if (visit.customer_gender && !customer?.gender)
        void queryClient.invalidateQueries({
          queryKey: ["customers", "one", visit.customer_id],
        });
      for (const ps of visit.package_sales)
        if (ps.beneficiary_customer_id)
          void queryClient.invalidateQueries({
            queryKey: ["customer-packages", ps.beneficiary_customer_id],
          });
      forgetDraft(visit.customer_id);
      showToast(
        savedSummary(visit) +
          visit.package_sales
            .map(
              (p) =>
                ` · bán ${p.description}${p.beneficiary_name ? ` cho ${p.beneficiary_name}` : ""}`,
            )
            .join("") +
          (visit.business_date !== todayIso
            ? ` · nhập bù ngày ${visit.business_date.split("-").reverse().join("/")}`
            : ""),
      );
      resetVisit();
    },
    visitFailed: async (error) => {
      if (error instanceof ApiError && error.status === 409 && customer) {
        // Another device used the session: show fresh counts, keep the other lines.
        await queryClient.invalidateQueries({
          queryKey: ["customer-packages", customer.id],
        });
      }
      showToast(messageFor(error), true);
    },
    saveCorrection: async (plan) => {
      if (!correcting) throw new Error("no visit to correct");
      const m = await loadCorrection();
      return m.saveCorrection(correcting.visit, plan, idempotencyKey);
    },
    correctionSaved: (saved) => {
      const old = correcting?.visit;
      void queryClient.invalidateQueries({ queryKey: ["visits"] });
      void queryClient.invalidateQueries({ queryKey: ["appointments"] });
      void queryClient.invalidateQueries({ queryKey: ["customer-packages"] });
      void invalidateManagementViews(queryClient, saved.visit.customer_id);
      if (old && old.customer_id !== saved.visit.customer_id)
        void invalidateManagementViews(queryClient, old.customer_id);
      showToast(saved.summary);
      resetVisit();
    },
    correctionFailed: (error) => {
      if (error instanceof ApiError && error.status === 409) {
        void queryClient.invalidateQueries({ queryKey: ["visits"] });
        void queryClient.invalidateQueries({ queryKey: ["customer-packages"] });
      }
      showToast(
        fieldMessage(error, "check_in_at") ??
          fieldMessage(error, "reason") ??
          messageFor(error),
        true,
      );
    },
    // A cart with packages and no service is a plain package sale (spec §10.4.1, US10): one
    // package → `POST /sales`; several (EM 2026-11-07 item 4) → `POST /sales/checkout`, one save
    // and one payment for all of them.
    sellPackages: async (drafts) => {
      if (!customer) throw new Error("no customer");
      const m = await loadCartPackages();
      return m.sellCart(
        customer,
        drafts,
        paymentsFor(payable, mode, cashPart),
        idempotencyKey,
      );
    },
    packagesSold: (soldText, drafts) => {
      void queryClient.invalidateQueries({ queryKey: ["customer-packages"] });
      // The day list shows package-only sales under the visits (F6).
      void queryClient.invalidateQueries({ queryKey: ["sales", "day"] });
      void invalidateManagementViews(queryClient, customer?.id);
      for (const d of drafts)
        if (d.beneficiary)
          void invalidateManagementViews(queryClient, d.beneficiary.id);
      forgetDraft(customer?.id);
      showToast(soldText);
      resetVisit();
    },
    packagesFailed: (error) => {
      showToast(messageFor(error), true);
    },
  };
  useEffect(() => {
    savesRef.current = saves;
  });

  return (
    <>
      {(correcting || appointment || (restored && customer)) && (
        <Suspense fallback={null}>
          <CartContextBars
            correcting={correcting?.visit ?? null}
            onStopCorrecting={resetVisit}
            appointment={appointment}
            today={todayIso}
            restoredFor={
              restored && !correcting ? (customer?.full_name ?? null) : null
            }
            onUnlink={() => {
              setAppointment(null);
              setServiceDay("");
              setServiceTime("");
            }}
            onDiscard={() => {
              if (!customer) return;
              forgetDraft(customer.id);
              select(customer, true);
              setFocusDeduct(false);
            }}
          />
        </Suspense>
      )}
      {!customer && (
        <p className="empty-note">Chọn khách để bắt đầu ghi lượt.</p>
      )}
      {customer && lines.length === 0 && sales.length === 0 && (
        <p className="empty-note">
          Chưa có dịch vụ. Bấm “Trừ 1 buổi” ở gói liệu trình, chọn dịch vụ lẻ
          bên dưới, hoặc bán gói liệu trình.
        </p>
      )}
      {lines.length > 0 && (
        <Suspense fallback={null}>
          <CartLines
            lines={lines}
            onLines={setLines}
            covered={covered}
            planName={planOf}
            attempted={attempted}
            staff={staff}
            staffPending={staffPending}
            technicians={technicians}
            staffSelects={staffSelects}
            focusLineKey={focusLineKey}
            openKey={openItem}
            onOpen={setOpenItem}
          />
        </Suspense>
      )}

      {customer && (
        <Suspense fallback={addOneOffBox}>
          <AddOneOff
            key={customer.id}
            customer={customer}
            lines={lines}
            onAdd={(service, priceVariant, price, categoryName) => {
              addLine({
                key: nextKey.current++,
                source: "one_off",
                serviceId: service.id,
                serviceName: service.name,
                ...(categoryName ? { categoryName } : {}),
                priceVariant,
                price,
                staffId: appointment?.staff_id ?? "",
              });
            }}
            onProblem={(m) => {
              showToast(m, true);
            }}
          />
        </Suspense>
      )}

      {customer && !correcting && can(me, "package.sell") && (
        <Suspense fallback={packagesBox}>
          <PackageSales
            sales={sales}
            onSales={setSales}
            lines={lines}
            covered={covered}
            staff={staff}
            customerId={customer.id}
            canDiscount={
              can(me, "sale.discount") || can(me, "sale.discount_unlimited")
            }
            unlimitedDiscount={can(me, "sale.discount_unlimited")}
            attempted={attempted}
            serverError={mutation.error}
            openPlanId={openPlan}
            onOpened={clearOpenPlan}
            ownerNamed={customer.named}
            openRef={openItem}
            onOpenRef={setOpenItem}
            onLines={setLines}
            canWriteNote={can(me, "customer.health_notes.write")}
            onTodaySession={(saleRef, s) => {
              addLine({
                key: nextKey.current++,
                source: "new_package",
                saleRef,
                ...s,
                staffId: appointment?.staff_id ?? "",
              });
            }}
          />
        </Suspense>
      )}
      {companions.length > 0 && (
        <p className="field-hint">
          Người đi cùng (ghi chú của lịch hẹn): {companions.join(", ")}
        </p>
      )}

      {customer && (lines.length > 0 || sales.length > 0) && (
        <>
          <div className="cart-options">
            <Suspense fallback={null}>
              {lines.length > 0 && (
                <ServiceTime
                  mayBackdate={mayBackdate}
                  today={todayIso}
                  day={serviceDay}
                  time={serviceTime}
                  invalid={attempted && problems.time}
                  onDay={(d) => {
                    setServiceDay(d === todayIso ? "" : d);
                    if (d !== todayIso && serviceTime === "")
                      setServiceTime(nowHm());
                  }}
                  onTime={setServiceTime}
                />
              )}
              {due > 0 &&
                (can(me, "sale.discount") ||
                  can(me, "sale.discount_unlimited")) && (
                <DiscountField
                  due={due}
                  draft={discount}
                  onChange={setDiscount}
                  overCap={
                    discountAmount > 0 &&
                    exceedsCap(
                      due,
                      discountAmount,
                      can(me, "sale.discount_unlimited"),
                    )
                  }
                />
              )}
            </Suspense>
          </div>
          {payable > 0 && (
            <Suspense fallback={null}>
              <PaymentChoices
                name="visit-payment"
                mode={mode}
                onMode={setMode}
                cashText={cashText}
                onCashText={setCashText}
                total={payable}
                invalid={attempted && problems.cash}
              />
            </Suspense>
          )}
          {can(me, "customer.health_notes.write") && lines.length > 0 && (
            <label className="field">
              <span>
                Ghi chú buổi{" "}
                <span className="font-normal text-text-muted">
                  (không bắt buộc)
                </span>
              </span>
              <textarea
                value={note}
                onChange={(e) => {
                  setNote(e.target.value);
                }}
                rows={2}
                maxLength={2000}
                className="input"
              />
            </label>
          )}
          {correcting && correctionDraft && (
            <Suspense fallback={null}>
              <CorrectionPanel
                visit={correcting.visit}
                oldNote={correcting.note}
                draft={correctionDraft}
                invalid={attempted && reason.trim() === ""}
                onReason={setReason}
              />
            </Suspense>
          )}
          {(mutation.isError || correction.isError) &&
            !(mutation.error instanceof ApiError) &&
            !(correction.error instanceof ApiError) && (
              <p role="alert" className="alert">
                Chưa lưu được. Bấm Lưu để thử lại (không bị ghi đôi).
              </p>
            )}
          <div role="group" aria-label="Lưu lượt" className="checkout">
            {correcting ? (
              <Suspense fallback={null}>
                <CorrectionTotal
                  total={payable}
                  paid={correcting.visit.total}
                />
              </Suspense>
            ) : (
              <p className="checkout-total" aria-live="polite">
                <span>Phải trả</span>
                <span className="checkout-amount">
                  {discountAmount > 0 && <s>{formatVnd(due)}</s>}
                  {formatVnd(payable)}
                </span>
              </p>
            )}
            {packagesTotal > 0 && (
              <p className="checkout-split">
                Dịch vụ lẻ {formatVnd(due - discountAmount)} + gói{" "}
                {formatVnd(packagesTotal)}
              </p>
            )}
            {payable === 0 && sales.length === 0 && (
              <p className="checkout-split">
                Lượt này chỉ trừ buổi liệu trình, không phát sinh tiền.
              </p>
            )}
            <button
              type="button"
              onClick={save}
              aria-disabled={saving}
              aria-busy={saving}
              className="btn btn-primary btn-lg btn-block"
            >
              {saving
                ? "Đang lưu…"
                : correcting
                  ? "Lưu sửa lượt"
                  : lines.length === 0
                    ? "Lưu hóa đơn gói"
                    : "Lưu lượt khách"}
            </button>
          </div>
        </>
      )}
    </>
  );
}
