import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { messageFor } from "../../lib/errors";
import { ErrorState, RowsSkeleton } from "../reports/ui";
import { evaluationsQuery, invalidateStaff, saveEvaluation, type EvaluationSheet, type StaffEvaluation } from "./api";
import { LEVELS, LEVEL_LABEL, LEVEL_SHORT, monthLabel, type Level } from "./labels";
import { ClosedBanner } from "./ui";

/**
 * "Đánh giá tháng": the month's 100-point scorecard per person. Daily praise/complaints live in
 * the daily book; the book prompts for this step only while it is due (spec §10 AC7.4).
 */
export function EvaluationTab({ month }: { month: string }) {
  const evals = useQuery(evaluationsQuery(month));
  const [scoring, setScoring] = useState<StaffEvaluation | null>(null);

  if (evals.isPending) return <RowsSkeleton rows={6} />;
  if (evals.isError) return <ErrorState error={evals.error} onRetry={() => void evals.refetch()} />;
  const sheet = evals.data;
  const missing = sheet.staff.filter((s) => !s.evaluation).length;

  return (
    <div className="grid content-start gap-4">
      {sheet.closed && <ClosedBanner />}
      <p role="status" className="text-sm">
        {missing === 0 ? (
          <strong className="text-success">
            ✓ Đã đánh giá đủ {sheet.staff.length} người · {monthLabel(sheet.month)}
          </strong>
        ) : (
          <>
            {monthLabel(sheet.month)}: còn <strong>{missing}</strong>/{sheet.staff.length} người chưa đánh giá. Chấm vào cuối tháng hoặc đầu
            tháng sau, trước khi chốt lương.
          </>
        )}
      </p>
      <ul className="card eval-list">
        {sheet.staff.map((s) => (
          <li key={s.staff_id} className="eval-row">
            <div className="min-w-0 flex-1">
              <p className="font-semibold">{s.full_name}</p>
              <p className="text-sm text-text-muted">
                {s.compliments} khen · {s.complaints} phàn nàn · nghỉ {s.attendance.leave_p + s.attendance.leave_dx + s.attendance.absent_k} ngày ·
                muộn {s.attendance.late_count} lần
              </p>
            </div>
            {s.evaluation ? (
              <span className={`badge ${s.evaluation.result === "fail" ? "badge-danger" : s.evaluation.result === "m3" ? "badge-warn" : "badge-ok"}`}>
                {s.evaluation.total_points} điểm · {LEVEL_SHORT[s.evaluation.result]}
              </span>
            ) : (
              <span className="badge badge-warn">Chưa đánh giá</span>
            )}
            {!sheet.closed && (
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => {
                  setScoring(s);
                }}
              >
                {s.evaluation ? "Chấm lại" : "Chấm điểm"}
              </button>
            )}
          </li>
        ))}
      </ul>
      {scoring && (
        <ScoreSheet
          sheet={sheet}
          staff={scoring}
          onClose={() => {
            setScoring(null);
          }}
        />
      )}
    </div>
  );
}

const PERCENT: Record<Level, number> = { m1: 100, m2: 80, m3: 60, fail: 30 };

function ScoreSheet({ sheet, staff, onClose }: { sheet: EvaluationSheet; staff: StaffEvaluation; onClose: () => void }) {
  const queryClient = useQueryClient();
  const initial = Object.fromEntries(sheet.criteria.map((c) => [c.key, staff.evaluation?.criteria.find((x) => x.key === c.key)?.level ?? "m1"])) as Record<string, Level>;
  const [levels, setLevels] = useState(initial);
  const [serious, setSerious] = useState(staff.evaluation?.serious_issue ?? false);
  const [plan, setPlan] = useState(staff.evaluation?.coaching_plan ?? "");
  const [comment, setComment] = useState(staff.evaluation?.comment ?? "");
  const save = useMutation({
    mutationFn: saveEvaluation,
    onSuccess: async () => {
      await invalidateStaff(queryClient);
      onClose();
    },
  });
  const total = sheet.criteria.reduce((sum, c) => sum + Math.floor((c.max * PERCENT[levels[c.key] ?? "m1"]) / 100), 0);
  const byTotal: Level = total >= 85 ? "m1" : total >= 70 ? "m2" : total >= 50 ? "m3" : "fail";
  const result: Level = serious && (byTotal === "m1" || byTotal === "m2") ? "m3" : byTotal;
  const needsPlan = result === "m3" || result === "fail";
  const formId = "score-form";
  return (
    <BottomSheet
      open
      wide
      onClose={onClose}
      title={`Đánh giá ${staff.full_name}`}
      description={`${total} điểm · ${LEVEL_LABEL[result]}`}
      footer={
        <button
          type="submit"
          form={formId}
          className="btn btn-primary"
          aria-busy={save.isPending || undefined}
          disabled={needsPlan && !plan.trim()}
        >
          Lưu đánh giá
        </button>
      }
    >
      <form
        id={formId}
        className="grid gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (save.isPending) return;
          save.mutate({
            staff_id: staff.staff_id,
            month: sheet.month,
            criteria: sheet.criteria.map((c) => ({ key: c.key, level: levels[c.key] ?? "m1" })),
            serious_issue: serious,
            coaching_plan: plan.trim() || null,
            comment: comment.trim() || null,
          });
        }}
      >
        <div className="grid gap-3 sm:grid-cols-2">
          {sheet.criteria.map((c) => (
            <label key={c.key} className="filter-field">
              {c.label} (tối đa {c.max})
              <select
                value={levels[c.key]}
                onChange={(e) => {
                  setLevels({ ...levels, [c.key]: e.target.value as Level });
                }}
              >
                {LEVELS.map((l) => (
                  <option key={l} value={l}>
                    {LEVEL_LABEL[l]} · {Math.floor((c.max * PERCENT[l]) / 100)} điểm
                  </option>
                ))}
              </select>
            </label>
          ))}
        </div>
        <label className="choice">
          <input
            type="checkbox"
            checked={serious}
            onChange={(e) => {
              setSerious(e.target.checked);
            }}
          />
          <span>Có khiếu nại nghiêm trọng hoặc vi phạm nội quy nhiều lần (kết quả tối đa Mức 3)</span>
        </label>
        <label className="filter-field">
          Kế hoạch kèm cặp tháng sau{needsPlan ? " (bắt buộc)" : ""}
          <textarea
            rows={2}
            maxLength={1000}
            required={needsPlan}
            value={plan}
            onChange={(e) => {
              setPlan(e.target.value);
            }}
          />
        </label>
        <label className="filter-field">
          Nhận xét của quản lý
          <textarea
            rows={2}
            maxLength={1000}
            value={comment}
            onChange={(e) => {
              setComment(e.target.value);
            }}
          />
        </label>
        {save.isError && (
          <p role="alert" className="text-sm text-danger">
            {messageFor(save.error)}
          </p>
        )}
      </form>
    </BottomSheet>
  );
}
