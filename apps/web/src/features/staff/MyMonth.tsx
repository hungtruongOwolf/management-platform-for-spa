import { useQuery } from "@tanstack/react-query";
import { EmptyState, ErrorState, RowsSkeleton } from "../reports/ui";
import { myMonthQuery } from "./api";
import { LEVEL_LABEL, STATUS_CODE, STATUS_LABEL } from "./labels";
import { PayslipView } from "./PayrollTab";
import { ClosedBanner, dayLabel, vnd } from "./ui";

/**
 * "Tháng của tôi": a technician's own attendance, reviews, evaluation, bonuses/penalties and
 * payslip (`GET /me/staff`). The server picks the person from the session; nothing here can ask
 * for someone else.
 */
export function MyMonth({ month }: { month: string }) {
  const query = useQuery(myMonthQuery(month));
  if (query.isPending) return <RowsSkeleton rows={5} />;
  if (query.isError) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />;
  const data = query.data.data;
  if (!query.data.linked || !data) {
    return (
      <EmptyState title="Tài khoản chưa gắn với hồ sơ nhân viên">
        Nhờ chủ spa gắn tài khoản này vào hồ sơ của bạn để xem ngày công, đánh giá và phiếu lương.
      </EmptyState>
    );
  }
  const t = data.attendance.totals;
  return (
    <div className="grid content-start gap-6">
      {data.closed && <ClosedBanner />}
      <section className="grid gap-3" aria-labelledby="my-attendance">
        <h2 id="my-attendance" className="section-title">
          Ngày công
        </h2>
        <p className="text-sm text-text-muted">
          Đi làm {t.present} ngày · muộn {t.late_count} lần ({t.late_minutes} phút) · P {t.leave_p} · ĐX {t.leave_dx} · K {t.absent_k}
        </p>
        {data.attendance.days.length === 0 ? (
          <p className="text-text-muted">Chưa có ngày nào được chấm.</p>
        ) : (
          <ul className="grid gap-1">
            {data.attendance.days.map((d) => (
              <li key={d.date} className="flex flex-wrap gap-x-3 text-sm">
                <span className="w-28 tabular">{dayLabel(d.date)}</span>
                <span aria-label={STATUS_LABEL[d.status]}>
                  {STATUS_CODE[d.status]} {STATUS_LABEL[d.status]}
                </span>
                {d.check_in && <span className="text-text-muted">đến {d.check_in}</span>}
                {d.late_minutes > 0 && <span className="text-danger">muộn {d.late_minutes} phút</span>}
              </li>
            ))}
          </ul>
        )}
      </section>
      <section className="grid gap-2" aria-labelledby="my-evaluation">
        <h2 id="my-evaluation" className="section-title">
          Đánh giá
        </h2>
        <p className="text-sm">
          {data.reviews.reduce((n, r) => n + r.compliments, 0)} lần được khen · {data.reviews.reduce((n, r) => n + r.complaints, 0)} lần bị phàn nàn
        </p>
        {data.evaluation ? (
          <p>
            Tháng này: <strong>{data.evaluation.total_points} điểm · {LEVEL_LABEL[data.evaluation.result]}</strong>
            {data.evaluation.comment && <span className="block text-sm text-text-muted">{data.evaluation.comment}</span>}
          </p>
        ) : (
          <p className="text-text-muted">Chưa có đánh giá tháng (quản lý chấm ngày 1–5 tháng sau).</p>
        )}
        {data.adjustments.length > 0 && (
          <ul className="grid gap-1 text-sm">
            {data.adjustments.map((a) => (
              <li key={a.id}>
                {a.kind === "bonus" ? "Thưởng" : "Phạt"} {a.amount != null ? vnd(a.amount) : ""} {a.points != null ? `${String(a.points)} điểm` : ""} · {a.reason}
              </li>
            ))}
          </ul>
        )}
      </section>
      <section className="grid gap-2" aria-labelledby="my-payslip">
        <h2 id="my-payslip" className="section-title">
          Phiếu lương
        </h2>
        {data.payslip ? <PayslipView slip={data.payslip} closed={data.closed} /> : <p className="text-text-muted">Chủ spa chưa nhập lương cho bạn.</p>}
      </section>
    </div>
  );
}
