import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { ErrorState, RowsSkeleton } from "../reports/ui";
import { myEvaluationHistoryQuery, type HistoryMonth } from "./api";
import { MonthsHistory, ScoreCard } from "./EvaluationViews";
import { monthLabel } from "./labels";

/**
 * "Lịch sử đánh giá" in "Công và lương của tôi" (EM 2026-11-08): the signed-in staff member's own
 * months — the same trend and rows as the managers' history — from `GET /me/evaluations/history`
 * (the session's own staff link; former staff: only the months they worked). "Xem phiếu" opens
 * their own scorecard read-only; "Từng ngày" moves the page to that month. No rescoring, no notes
 * journal. Loaded on demand (lazy) from MyMonth.
 */
export function MyEvaluationHistory({ onMonth }: { onMonth: (month: string) => void }) {
  const history = useQuery(myEvaluationHistoryQuery);
  const [open, setOpen] = useState<HistoryMonth | null>(null);
  if (history.isPending) return <RowsSkeleton rows={4} />;
  if (history.isError) return <ErrorState error={history.error} onRetry={() => void history.refetch()} />;
  const data = history.data.data;
  if (!data) return <p className="text-sm text-text-muted">Tài khoản chưa gắn với hồ sơ nhân viên nào.</p>;
  return (
    <div className="grid gap-4">
      <p className="text-sm text-text-muted">
        {monthLabel(data.from)} – {monthLabel(data.to).toLowerCase()}
      </p>
      <MonthsHistory
        name="bạn"
        months={data.months}
        onScorecard={setOpen}
        empty="Chưa có tháng làm việc nào trong khoảng này."
        actions={(m) => (
          <button
            type="button"
            className="btn btn-ghost"
            aria-label={`Từng ngày ${monthLabel(m.month).toLowerCase()}`}
            onClick={() => {
              onMonth(m.month);
            }}
          >
            Từng ngày
          </button>
        )}
      />
      {open?.evaluation && (
        <ScoreCard
          name="của bạn"
          month={open.month}
          evaluation={open.evaluation}
          onClose={() => {
            setOpen(null);
          }}
        />
      )}
    </div>
  );
}
