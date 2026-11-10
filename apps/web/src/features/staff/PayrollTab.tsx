import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { TimeField } from "../../components/TimeField";
import { ResponsiveTable, type Column } from "../../components/ResponsiveTable";
import { ApiError } from "../../lib/api";
import { fieldMessage, messageFor } from "../../lib/errors";
import { EmptyState, ErrorState, RowsSkeleton } from "../reports/ui";
import {
  closePayroll,
  invalidateStaff,
  payrollQuery,
  rulesQuery,
  saveRules,
  type LeaverRefund,
  type MissingItem,
  type MissingPayrollData,
  type Payslip,
  type PayrollRules,
} from "./api";
import { FLAG_LABEL, LEVEL_SHORT, monthLabel, percentOfBp } from "./labels";
import { slipColumns, totalColumns, type PayColumns } from "./payrollColumns";
import { serviceWork, upsellWork } from "./payslipWork";
import { signedVnd, vnd } from "./ui";
import { WorkTable } from "./WorkTable";

/**
 * Bảng lương (payroll.read): one row per person — base, deductions, commission, meal allowance,
 * bonuses − penalties, net, which the other columns add up to — and each payslip. An open month is "tạm tính" (live); closing freezes every payslip and locks the
 * month's records (payroll.close).
 */
export function PayrollTab({ month, canClose }: { month: string; canClose: boolean }) {
  const query = useQuery(payrollQuery(month));
  const [slip, setSlip] = useState<Payslip | null>(null);
  const [closing, setClosing] = useState(false);
  const [rules, setRules] = useState(false);

  if (query.isPending) return <RowsSkeleton rows={6} />;
  if (query.isError) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />;
  const p = query.data;
  const total = totalColumns(p.totals);
  const columns: Column<Payslip>[] = [
    {
      id: "name",
      header: "Nhân viên",
      card: "title",
      width: "14%",
      cell: (s) => (
        <button
          type="button"
          className="name-link"
          onClick={() => {
            setSlip(s);
          }}
        >
          {s.full_name}
        </button>
      ),
    },
    { id: "position", header: "Vị trí", card: "subtitle", width: "10%", cell: (s) => s.position },
    // Every money column of a row adds up to its Thực lĩnh (payrollColumns.ts).
    { id: "base", header: "Lương cứng", numeric: true, width: "10%", cell: (s) => vnd(slipColumns(s).base) },
    { id: "deduct", header: "Trừ nghỉ, muộn", numeric: true, width: "11%", cell: (s) => deductionCell(slipColumns(s).deductions) },
    { id: "commission", header: "Hoa hồng", numeric: true, width: "10%", cell: (s) => vnd(slipColumns(s).commission) },
    { id: "meal", header: "Tiền ăn", numeric: true, width: "9%", cell: (s) => vnd(slipColumns(s).meal) },
    { id: "adjust", header: "Thưởng − phạt", numeric: true, width: "10%", cell: (s) => signedVnd(slipColumns(s).adjustments) },
    { id: "net", header: "Thực lĩnh", numeric: true, card: "aside", width: "11%", cell: (s) => <NetCell columns={slipColumns(s)} /> },
    {
      id: "flags",
      header: "Lưu ý",
      width: "15%",
      cell: (s) => (s.flags.length === 0 ? "–" : s.flags.map((f) => FLAG_LABEL[f]).join(" · ")),
    },
  ];

  return (
    <div className="grid content-start gap-4">
      <div className="flex flex-wrap items-center gap-3">
        <span className={`badge ${p.closed ? "badge-muted" : "badge-info"}`}>{p.closed ? "Đã chốt" : "Tạm tính"}</span>
        {p.closed && p.closed_by_name && <span className="text-sm text-text-muted">Chốt bởi {p.closed_by_name}</span>}
        <span className="flex-1" />
        {canClose && (
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => {
              setRules(true);
            }}
          >
            Quy tắc lương
          </button>
        )}
        {canClose && !p.closed && p.ended && (
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => {
              setClosing(true);
            }}
          >
            Chốt lương {monthLabel(p.month).toLowerCase()}
          </button>
        )}
      </div>
      {p.payslips.length === 0 ? (
        <EmptyState title="Chưa có nhân viên nào trong tháng này">
          Thêm người ở tab “Hồ sơ nhân viên”, rồi chấm công ở “Sổ hằng ngày”: bảng lương tự tính từ đó.
        </EmptyState>
      ) : (
        <div className="card overflow-hidden">
        <ResponsiveTable
          caption={`Bảng lương ${monthLabel(p.month)}`}
          rows={p.payslips}
          rowKey={(s) => s.staff_id}
          columns={columns}
          cardsBelow={768}
          footer={{
            table: (
              <tr>
                <th scope="row" colSpan={2}>
                  Tổng {p.totals.staff} người
                </th>
                <td className="num">{vnd(total.base)}</td>
                <td className="num">{deductionCell(total.deductions)}</td>
                <td className="num">{vnd(total.commission)}</td>
                <td className="num">{vnd(total.meal)}</td>
                <td className="num">{signedVnd(total.adjustments)}</td>
                <td className="num">
                  <NetCell columns={total} />
                </td>
                <td />
              </tr>
            ),
            cards: (
              <div className="rt-card text-right font-semibold tabular">
                Tổng thực lĩnh <NetCell columns={total} />
              </div>
            ),
          }}
        />
        </div>
      )}
      {slip && (
        <PayslipSheet
          slip={slip}
          closed={p.closed}
          onClose={() => {
            setSlip(null);
          }}
        />
      )}
      {p.leaver_refunds.length > 0 && <LeaverRefunds refunds={p.leaver_refunds} />}
      {closing && (
        <CloseSheet
          month={p.month}
          missing={p.missing}
          incomplete={p.payslips.some((s) => s.flags.includes("unrecorded_days"))}
          onClose={() => {
            setClosing(false);
          }}
        />
      )}
      {rules && (
        <RulesSheet
          onClose={() => {
            setRules(false);
          }}
        />
      )}
    </div>
  );
}

function deductionCell(amount: number): string {
  return amount > 0 ? `−${vnd(amount)}` : "–";
}

/**
 * Thực lĩnh of a row. When deductions exceeded the month's pay, net pay stays at 0đ (ADR-0019 D3)
 * and the amount not deducted is said here, so the row's columns still visibly add up to it.
 */
function NetCell({ columns }: { columns: PayColumns }) {
  return (
    <>
      <strong>{vnd(columns.net)}</strong>
      {columns.shortfall > 0 && (
        <span className="block text-xs font-normal text-text-muted">+{vnd(columns.shortfall)} trừ vượt lương, không trừ</span>
      )}
    </>
  );
}

function Row({ label, value, strong = false, hint }: { label: string; value: string; strong?: boolean; hint?: string | undefined }) {
  return (
    <div className="payslip-row">
      <dt>
        {label}
        {hint && <span className="block text-xs text-text-muted">{hint}</span>}
      </dt>
      <dd className={`tabular ${strong ? "font-semibold" : ""}`}>{value}</dd>
    </div>
  );
}

/**
 * How the month's base was reached: the day rate always (it is on the full monthly salary), and
 * the employed share when the person did not work the whole month (ADR-0019 D4).
 */
function baseHint(slip: Payslip): string {
  const dayRate = `Lương 1 ngày ${vnd(slip.day_rate)} = lương cứng ÷ (${String(slip.days_in_month)} − ${String(slip.free_leave_days)})`;
  if (slip.base_pay === slip.base_salary) return dayRate;
  return `${vnd(slip.base_salary)} × ${String(slip.employed_days)}/${String(slip.days_in_month)} ngày làm việc, làm tròn đến nghìn đồng. ${dayRate}`;
}

/** Phiếu lương: every line the sheet had, so a technician can check each amount. */
export function PayslipView({ slip, closed, showFlags = true }: { slip: Payslip; closed: boolean; showFlags?: boolean }) {
  const a = slip.attendance;
  return (
    <div className="payslip-view grid gap-4">
      <p className="text-sm text-text-muted">
        {closed ? "Đã chốt" : "Tạm tính, còn thay đổi đến khi chốt"} · {monthLabel(slip.month)} · {slip.employed_days}/{slip.days_in_month} ngày làm việc
      </p>
      {/* What still needs someone, up front with the status (EM 2026-11-07: not alone at the end). */}
      {showFlags && slip.flags.length > 0 && (
        <ul className="flex flex-wrap gap-2" aria-label="Cần xem">
          {slip.flags.map((f) => (
            <li key={f} className="badge badge-warn">
              {FLAG_LABEL[f]}
            </li>
          ))}
        </ul>
      )}
      <dl className="payslip">
        <Row label="Lương cứng" value={vnd(slip.base_pay)} hint={baseHint(slip)} />
        <Row
          label="Trừ ngày nghỉ"
          value={slip.leave_deduction > 0 ? `−${vnd(slip.leave_deduction)}` : "0đ"}
          hint={`Nghỉ có phép ${String(a.leave_p)} · đột xuất ${String(a.leave_dx)} · không phép ${String(a.absent_k)}; được nghỉ ${String(slip.free_leave_days)} ngày; trừ ${String(slip.deducted_leave_days)} ngày`}
        />
        <Row
          label="Trừ đi muộn"
          value={slip.late_deduction > 0 ? `−${vnd(slip.late_deduction)}` : "0đ"}
          hint={`Muộn ${String(a.late_count)} lần, ${String(a.late_minutes)} phút`}
        />
        <Row
          label="Hoa hồng làm liệu trình"
          value={vnd(slip.service_commission)}
          hint={`${percentOfBp(slip.commission_rate_bp)} × ${vnd(slip.service_commission_base)}`}
        />
        <Row
          label="Hoa hồng giới thiệu / chốt"
          value={vnd(slip.closing_commission)}
          hint={`${percentOfBp(slip.commission_rate_bp)} × ${vnd(slip.closing_commission_base)} (gói ${vnd(slip.upsell_package_base ?? slip.closing_commission_base)} · dịch vụ lẻ ${vnd(slip.upsell_service_base ?? 0)})`}
        />
        <Row label="Hỗ trợ tiền ăn" value={vnd(slip.meal_allowance)} />
        <Row label="Thưởng" value={vnd(slip.bonuses)} hint={slip.bonus_points ? `${String(slip.bonus_points)} điểm thưởng` : undefined} />
        <Row label="Phạt" value={slip.penalties > 0 ? `−${vnd(slip.penalties)}` : "0đ"} hint={slip.penalty_points ? `${String(slip.penalty_points)} điểm phạt` : undefined} />
        {(slip.net_pay_shortfall ?? 0) > 0 && (
          <Row
            label="Khoản trừ vượt lương (không trừ)"
            value={vnd(slip.net_pay_shortfall ?? 0)}
            hint="Thực lĩnh không bao giờ âm: phần trừ vượt quá lương của tháng do spa chịu."
          />
        )}
        <Row label="Thực lĩnh" value={vnd(slip.net_pay)} strong />
      </dl>
      <dl className="payslip payslip-muted" aria-label="Ghi nhận, chưa trả trong tháng">
        <Row label="Thưởng gắn bó ghi nhận" value={vnd(slip.loyalty_bonus_accrued)} hint="Trả khi tới mốc 12/24 tháng" />
        <Row
          label="Mức Tết của tháng"
          value={slip.tet_level ? `${LEVEL_SHORT[slip.tet_level]} · ${vnd(slip.tet_amount)}` : "Chưa đánh giá"}
          hint={slip.evaluation_level ? `Đánh giá ${LEVEL_SHORT[slip.evaluation_level]}; mức thấp hơn giữa ngày nghỉ và đánh giá` : undefined}
        />
      </dl>
      <WorkTable
        title="Dịch vụ đã làm"
        table={serviceWork(slip)}
        itemHeader="Dịch vụ"
        qtyHeader="Lượt"
        commissionLabel="Thành hoa hồng làm"
      />
      <WorkTable
        title="Giới thiệu / chốt được"
        table={upsellWork(slip)}
        itemHeader="Gói / dịch vụ"
        qtyHeader="Số lượng"
        commissionLabel="Thành hoa hồng giới thiệu / chốt"
      />
    </div>
  );
}

function PayslipSheet({ slip, closed, onClose }: { slip: Payslip; closed: boolean; onClose: () => void }) {
  return (
    <BottomSheet open wide onClose={onClose} title={`Phiếu lương ${slip.full_name}`} description={slip.position}>
      {/* A read-only sheet has no control to tab to: the region itself takes focus so keyboard users can scroll it. */}
      <div tabIndex={0} role="region" aria-label={`Chi tiết phiếu lương ${slip.full_name}`}>
        <PayslipView slip={slip} closed={closed} />
      </div>
    </BottomSheet>
  );
}

/**
 * Refunds of packages whose closer no longer works in the month (ADR-0019 D2b): where each was
 * taken back, or that it was not (their last month was already closed). Never silently dropped.
 */
function LeaverRefunds({ refunds }: { refunds: LeaverRefund[] }) {
  return (
    <section className="card grid gap-2 p-4" aria-labelledby="leaver-refunds">
      <h2 id="leaver-refunds" className="font-semibold">
        Hoàn tiền gói của người đã nghỉ
      </h2>
      <ul className="grid gap-1 text-sm">
        {refunds.map((r) => (
          <li key={r.refund_id}>
            {r.full_name} · {r.description} · hoàn {vnd(r.amount)} ngày {dayLabel(r.refund_date)}:{" "}
            {r.deducted_in
              ? `trừ lại hoa hồng chốt ở phiếu lương ${monthLabel(r.deducted_in).toLowerCase()}`
              : "không trừ lại được (tháng làm cuối đã chốt lương), spa chịu"}
          </li>
        ))}
      </ul>
    </section>
  );
}

/** "2026-11-05" → "05/11/2026". */
function dayLabel(iso: string): string {
  const [y, m, d] = iso.split("-");
  return `${d ?? ""}/${m ?? ""}/${y ?? ""}`;
}

const MISSING_LABEL: Record<MissingItem, string> = { pay_terms: "chưa có lương cứng", evaluation: "chưa đánh giá tháng" };

function CloseSheet({
  month,
  missing,
  incomplete,
  onClose,
}: {
  month: string;
  missing: MissingPayrollData[];
  incomplete: boolean;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const [key] = useState(() => crypto.randomUUID());
  const [allow, setAllow] = useState(false);
  const close = useMutation({
    mutationFn: () => closePayroll(month, allow, key),
    onSuccess: async () => {
      await invalidateStaff(queryClient);
      onClose();
    },
    // Someone else's change made the month incomplete meanwhile: refresh the list shown above.
    onError: async (error) => {
      if (error instanceof ApiError && error.code === "payroll.missing_data") await invalidateStaff(queryClient);
    },
  });
  const blocked = missing.length > 0;
  return (
    <BottomSheet
      open
      onClose={onClose}
      title={`Chốt lương ${monthLabel(month).toLowerCase()}`}
      footer={
        <>
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            Để sau
          </button>
          <button
            type="button"
            className="btn btn-primary"
            aria-busy={close.isPending || undefined}
            disabled={blocked || (incomplete && !allow)}
            onClick={() => {
              if (!close.isPending) close.mutate();
            }}
          >
            Chốt lương
          </button>
        </>
      }
    >
      <div className="grid gap-3">
        <p>
          Chốt xong, phiếu lương được giữ nguyên. Chấm công, đánh giá và thưởng/phạt của tháng sẽ chỉ còn xem được, <strong>không mở lại được</strong>.
          Cần sửa thì ghi khoản điều chỉnh vào tháng sau.
        </p>
        {blocked && (
          <div role="alert" className="grid gap-1">
            <p className="font-semibold">Chưa chốt được: bổ sung cho những người sau trước (nếu không, phiếu lương bị giữ ở 0đ).</p>
            <ul className="list-disc pl-5 text-sm">
              {missing.map((m) => (
                <li key={m.staff_id}>
                  {m.full_name}: {m.missing.map((x) => MISSING_LABEL[x]).join(", ")}
                </li>
              ))}
            </ul>
            <p className="text-sm text-text-muted">Lương cứng nhập ở “Hồ sơ nhân viên”, đánh giá tháng ở “Đánh giá tháng”.</p>
          </div>
        )}
        {incomplete && (
          <label className="choice">
            <input
              type="checkbox"
              checked={allow}
              onChange={(e) => {
                setAllow(e.target.checked);
              }}
            />
            <span>Còn ngày chưa chấm công. Tôi vẫn muốn chốt (ngày chưa chấm không bị trừ lương).</span>
          </label>
        )}
        {close.isError && (
          <p role="alert" className="text-sm text-danger">
            {messageFor(close.error)}
          </p>
        )}
      </div>
    </BottomSheet>
  );
}

function RulesSheet({ onClose }: { onClose: () => void }) {
  const query = useQuery(rulesQuery);
  return (
    <BottomSheet open wide onClose={onClose} title="Quy tắc lương của spa" description="Áp dụng cho các tháng chưa chốt">
      {query.isPending ? <RowsSkeleton rows={4} /> : query.isError ? <ErrorState error={query.error} onRetry={() => void query.refetch()} /> : <RulesForm initial={query.data.rules} version={query.data.version} onDone={onClose} />}
    </BottomSheet>
  );
}

/** The server's bounds on the numeric rules (hr/src/rules.rs `PayrollRules::validate`). */
const RULE_MAX = { free_leave_days: 10, late_grace_minutes: 240, late_penalty_amount: 10_000_000 } as const;
type BoundedRule = keyof typeof RULE_MAX;
const RULE_MAX_LABEL: Record<BoundedRule, string> = {
  free_leave_days: `Tối đa ${String(RULE_MAX.free_leave_days)} ngày.`,
  late_grace_minutes: `Tối đa ${String(RULE_MAX.late_grace_minutes)} phút.`,
  late_penalty_amount: `Tối đa ${vnd(RULE_MAX.late_penalty_amount)}.`,
};

/**
 * `version` is what the form was read at: saving over someone else's later save is refused
 * (`412`, ADR-0019 D10) and the form asks to reopen it, so neither save is lost silently.
 */
function RulesForm({ initial, version, onDone }: { initial: PayrollRules; version: number; onDone: () => void }) {
  const queryClient = useQueryClient();
  const [r, setR] = useState(initial);
  // The version these values were read at, fixed while the form is open: a refetch behind it
  // must not turn the next save into an overwrite of someone else's.
  const [readAt] = useState(version);
  const save = useMutation({
    mutationFn: () => saveRules(r, readAt),
    onSuccess: async () => {
      await invalidateStaff(queryClient);
      onDone();
    },
    // Reopening then shows the newer rules (and their version) at once.
    onError: async (error) => {
      if (error instanceof ApiError && error.code === "conflict.stale_version") await invalidateStaff(queryClient);
    },
  });
  const stale = save.error instanceof ApiError && save.error.code === "conflict.stale_version";
  const kFree = r.free_leave_codes.includes("absent_k");
  const num = (v: string) => Number(v.replace(/\D/g, "") || "0");
  const tooHigh = (Object.keys(RULE_MAX) as BoundedRule[]).filter((f) => r[f] > RULE_MAX[f]);
  const ruleError = (f: BoundedRule) => (tooHigh.includes(f) || fieldMessage(save.error, f) ? RULE_MAX_LABEL[f] : undefined);
  const bounded = (f: BoundedRule, label: string) => (
    <label className="filter-field">
      {label}
      <input
        inputMode="numeric"
        value={r[f]}
        onChange={(e) => {
          setR({ ...r, [f]: num(e.target.value) });
        }}
        aria-invalid={ruleError(f) ? true : undefined}
      />
      {ruleError(f) && <span className="text-sm text-danger">{ruleError(f)}</span>}
    </label>
  );
  return (
    <form
      className="grid gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        if (!save.isPending && tooHigh.length === 0) save.mutate();
      }}
    >
      {bounded("free_leave_days", "Số ngày nghỉ không trừ lương mỗi tháng")}
      <label className="choice">
        <input
          type="checkbox"
          checked={kFree}
          onChange={(e) => {
            setR({ ...r, free_leave_codes: e.target.checked ? ["leave_p", "leave_dx", "absent_k"] : ["leave_p", "leave_dx"] });
          }}
        />
        <span>Nghỉ không phép cũng được tính vào số ngày không trừ lương (như file Excel)</span>
      </label>
      <div className="grid gap-4 sm:grid-cols-3">
        <label className="filter-field">
          Giờ vào ca mặc định
          <TimeField value={r.default_shift_start} onChange={(hm) => { setR({ ...r, default_shift_start: hm || "09:00" }); }} />
        </label>
        {bounded("late_grace_minutes", "Muộn dưới (phút) không tính")}
        {bounded("late_penalty_amount", "Phạt mỗi lần muộn (đ)")}
      </div>
      <label className="choice">
        <input type="checkbox" checked={r.commission_on_bonus_sessions} onChange={(e) => { setR({ ...r, commission_on_bonus_sessions: e.target.checked }); }} />
        <span>Tính hoa hồng cả buổi tặng của gói liệu trình</span>
      </label>
      <label className="choice">
        <input type="checkbox" checked={r.closing_commission} onChange={(e) => { setR({ ...r, closing_commission: e.target.checked }); }} />
        <span>Có hoa hồng chốt gói liệu trình cho người chốt</span>
      </label>
      <label className="filter-field">
        Thưởng gắn bó tháng vào làm hoặc nghỉ giữa chừng
        <select value={r.loyalty_partial_month} onChange={(e) => { setR({ ...r, loyalty_partial_month: e.target.value as PayrollRules["loyalty_partial_month"] }); }}>
          <option value="fifteen_days">Đủ cả tháng nếu làm từ 15 ngày trở lên, dưới 15 ngày không tính</option>
          <option value="prorate">Theo số ngày làm trong tháng</option>
          <option value="full">Đủ cả tháng</option>
          <option value="none_if_partial">Không tính</option>
        </select>
      </label>
      <label className="filter-field">
        Hỗ trợ tiền ăn
        <select value={r.meal_allowance_basis} onChange={(e) => { setR({ ...r, meal_allowance_basis: e.target.value as PayrollRules["meal_allowance_basis"] }); }}>
          <option value="monthly">Cố định mỗi tháng (tháng không đi làm ngày nào thì 0đ)</option>
          <option value="days_worked">Theo số ngày đi làm thực tế</option>
        </select>
      </label>
      {save.isError && (
        <p role="alert" className="text-sm text-danger">
          {stale ? "Quy tắc vừa được lưu ở máy khác. Đóng rồi mở lại “Quy tắc lương” để xem bản mới nhất, rồi sửa lại." : messageFor(save.error)}
        </p>
      )}
      <div className="flex justify-end">
        <button type="submit" className="btn btn-primary" aria-busy={save.isPending || undefined} disabled={tooHigh.length > 0}>
          Lưu quy tắc
        </button>
      </div>
    </form>
  );
}
