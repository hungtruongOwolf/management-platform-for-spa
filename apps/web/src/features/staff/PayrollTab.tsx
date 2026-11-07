import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { TimeField } from "../../components/TimeField";
import { ResponsiveTable, type Column } from "../../components/ResponsiveTable";
import { messageFor } from "../../lib/errors";
import { EmptyState, ErrorState, RowsSkeleton } from "../reports/ui";
import { closePayroll, invalidateStaff, payrollQuery, rulesQuery, saveRules, type Payslip, type PayrollRules } from "./api";
import { FLAG_LABEL, LEVEL_SHORT, monthLabel, percentOfBp } from "./labels";
import { serviceWork, upsellWork } from "./payslipWork";
import { vnd } from "./ui";
import { WorkTable } from "./WorkTable";

/**
 * Bảng lương (payroll.read): one row per person — base, deductions, commission, bonuses, net — and
 * each payslip. An open month is "tạm tính" (live); closing freezes every payslip and locks the
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
  const columns: Column<Payslip>[] = [
    {
      id: "name",
      header: "Nhân viên",
      card: "title",
      width: "16%",
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
    { id: "position", header: "Vị trí", card: "subtitle", width: "12%", cell: (s) => s.position },
    { id: "base", header: "Lương cứng", numeric: true, width: "11%", cell: (s) => vnd(s.base_salary) },
    { id: "deduct", header: "Trừ nghỉ, muộn", numeric: true, width: "12%", cell: (s) => (s.leave_deduction + s.late_deduction > 0 ? `−${vnd(s.leave_deduction + s.late_deduction)}` : "–") },
    { id: "commission", header: "Hoa hồng", numeric: true, width: "10%", cell: (s) => vnd(s.service_commission + s.closing_commission) },
    { id: "adjust", header: "Thưởng − phạt", numeric: true, width: "11%", cell: (s) => vnd(s.bonuses - s.penalties) },
    { id: "net", header: "Thực lĩnh", numeric: true, card: "aside", width: "11%", cell: (s) => <strong>{vnd(s.net_pay)}</strong> },
    {
      id: "flags",
      header: "Lưu ý",
      width: "17%",
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
          Thêm người ở tab “Hồ sơ”, rồi chấm công ở “Sổ hằng ngày”: bảng lương tự tính từ đó.
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
                <td className="num">{vnd(p.totals.base_salary)}</td>
                <td className="num">{p.totals.deductions > 0 ? `−${vnd(p.totals.deductions)}` : "–"}</td>
                <td className="num">{vnd(p.totals.commission)}</td>
                <td className="num">{vnd(p.totals.bonuses - p.totals.penalties)}</td>
                <td className="num">
                  <strong>{vnd(p.totals.net_pay)}</strong>
                </td>
                <td />
              </tr>
            ),
            cards: (
              <p className="rt-card text-right font-semibold tabular">
                Tổng thực lĩnh {vnd(p.totals.net_pay)}
              </p>
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
      {closing && (
        <CloseSheet
          month={p.month}
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
        <Row label="Lương cứng" value={vnd(slip.base_salary)} hint={`Lương 1 ngày ${vnd(slip.day_rate)} = lương cứng ÷ (${String(slip.days_in_month)} − ${String(slip.free_leave_days)})`} />
        <Row
          label="Trừ ngày nghỉ"
          value={slip.leave_deduction > 0 ? `−${vnd(slip.leave_deduction)}` : "0đ"}
          hint={`Nghỉ P ${String(a.leave_p)} · ĐX ${String(a.leave_dx)} · K ${String(a.absent_k)}; được nghỉ ${String(slip.free_leave_days)} ngày; trừ ${String(slip.deducted_leave_days)} ngày`}
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

function CloseSheet({ month, incomplete, onClose }: { month: string; incomplete: boolean; onClose: () => void }) {
  const queryClient = useQueryClient();
  const [key] = useState(() => crypto.randomUUID());
  const [allow, setAllow] = useState(false);
  const close = useMutation({
    mutationFn: () => closePayroll(month, allow, key),
    onSuccess: async () => {
      await invalidateStaff(queryClient);
      onClose();
    },
  });
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
            disabled={incomplete && !allow}
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
      {query.isPending ? <RowsSkeleton rows={4} /> : query.isError ? <ErrorState error={query.error} onRetry={() => void query.refetch()} /> : <RulesForm initial={query.data.rules} onDone={onClose} />}
    </BottomSheet>
  );
}

function RulesForm({ initial, onDone }: { initial: PayrollRules; onDone: () => void }) {
  const queryClient = useQueryClient();
  const [r, setR] = useState(initial);
  const save = useMutation({
    mutationFn: () => saveRules(r),
    onSuccess: async () => {
      await invalidateStaff(queryClient);
      onDone();
    },
  });
  const kFree = r.free_leave_codes.includes("absent_k");
  const num = (v: string) => Number(v.replace(/\D/g, "") || "0");
  return (
    <form
      className="grid gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        if (!save.isPending) save.mutate();
      }}
    >
      <label className="filter-field">
        Số ngày nghỉ không trừ lương mỗi tháng
        <input inputMode="numeric" value={r.free_leave_days} onChange={(e) => { setR({ ...r, free_leave_days: num(e.target.value) }); }} />
      </label>
      <label className="choice">
        <input
          type="checkbox"
          checked={kFree}
          onChange={(e) => {
            setR({ ...r, free_leave_codes: e.target.checked ? ["leave_p", "leave_dx", "absent_k"] : ["leave_p", "leave_dx"] });
          }}
        />
        <span>Nghỉ không phép (K) cũng được tính vào số ngày không trừ lương (như file Excel)</span>
      </label>
      <div className="grid gap-4 sm:grid-cols-3">
        <label className="filter-field">
          Giờ vào ca mặc định
          <TimeField value={r.default_shift_start} onChange={(hm) => { setR({ ...r, default_shift_start: hm || "09:00" }); }} />
        </label>
        <label className="filter-field">
          Muộn dưới (phút) không tính
          <input inputMode="numeric" value={r.late_grace_minutes} onChange={(e) => { setR({ ...r, late_grace_minutes: num(e.target.value) }); }} />
        </label>
        <label className="filter-field">
          Phạt mỗi lần muộn (đ)
          <input inputMode="numeric" value={r.late_penalty_amount} onChange={(e) => { setR({ ...r, late_penalty_amount: num(e.target.value) }); }} />
        </label>
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
          <option value="prorate">Theo số ngày làm trong tháng</option>
          <option value="full">Đủ cả tháng</option>
          <option value="none_if_partial">Không tính</option>
        </select>
      </label>
      {save.isError && (
        <p role="alert" className="text-sm text-danger">
          {messageFor(save.error)}
        </p>
      )}
      <div className="flex justify-end">
        <button type="submit" className="btn btn-primary" aria-busy={save.isPending || undefined}>
          Lưu quy tắc
        </button>
      </div>
    </form>
  );
}
