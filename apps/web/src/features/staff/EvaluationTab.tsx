import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { DateField } from "../../components/DateField";
import { messageFor } from "../../lib/errors";
import { ErrorState, RowsSkeleton } from "../reports/ui";
import {
  evaluationsQuery,
  invalidateStaff,
  reviewsQuery,
  saveEvaluation,
  saveReviews,
  type EvaluationSheet,
  type ReviewSheet,
  type StaffEvaluation,
} from "./api";
import { LEVELS, LEVEL_LABEL, LEVEL_SHORT, type Level } from "./labels";
import { ClosedBanner, dayLabel } from "./ui";

/**
 * Đánh giá: the sheet's daily Khen / Phàn nàn / Ghi chú for one day (list per person), then the
 * month's 100-point scorecard per person (done on days 1–5 of the next month).
 */
export function EvaluationTab({ month, day, today, onDay }: { month: string; day: string; today: string; onDay: (d: string) => void }) {
  const evals = useQuery(evaluationsQuery(month));
  const reviews = useQuery(reviewsQuery(month));
  const [scoring, setScoring] = useState<StaffEvaluation | null>(null);

  if (evals.isPending || reviews.isPending) return <RowsSkeleton rows={6} />;
  if (evals.isError) return <ErrorState error={evals.error} onRetry={() => void evals.refetch()} />;
  if (reviews.isError) return <ErrorState error={reviews.error} onRetry={() => void reviews.refetch()} />;
  const sheet = evals.data;
  const inMonth = day.startsWith(sheet.month) ? day : sheet.month === today.slice(0, 7) ? today : `${sheet.month}-01`;

  return (
    <div className="grid content-start gap-6">
      {sheet.closed && <ClosedBanner />}
      <section className="grid gap-3" aria-labelledby="daily-review-title">
        <div className="flex flex-wrap items-end justify-between gap-3">
          <h2 id="daily-review-title" className="section-title">
            Đánh giá hằng ngày
          </h2>
          <DateField
            label="Ngày"
            value={inMonth}
            min={`${sheet.month}-01`}
            max={today}
            onChange={(d) => {
              if (d) onDay(d);
            }}
          />
        </div>
        <DailyReviews date={inMonth} closed={sheet.closed} reviews={reviews.data} />
      </section>
      <section className="grid gap-3" aria-labelledby="monthly-eval-title">
        <h2 id="monthly-eval-title" className="section-title">
          Đánh giá năng lực tháng (100 điểm)
        </h2>
        <ul className="grid gap-2">
          {sheet.staff.map((s) => (
            <li key={s.staff_id} className="card flex flex-wrap items-center gap-3 p-3">
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
                <span className="badge badge-muted">Chưa đánh giá</span>
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
      </section>
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

function DailyReviews({ date, closed, reviews }: { date: string; closed: boolean; reviews: ReviewSheet }) {
  const queryClient = useQueryClient();
  const [error, setError] = useState("");
  const save = useMutation({
    mutationFn: saveReviews,
    onSuccess: async () => {
      setError("");
      await invalidateStaff(queryClient);
    },
    onError: (e) => {
      setError(messageFor(e));
    },
  });
  return (
    <>
      <ul className="grid gap-2" aria-label={`Khen và phàn nàn ${dayLabel(date)}`}>
        {reviews.staff.map((s) => {
          const r = s.days.find((d) => d.date === date);
          const c = r?.compliments ?? 0;
          const k = r?.complaints ?? 0;
          const put = (compliments: number, complaints: number, note = r?.note ?? null) => {
            save.mutate([{ staff_id: s.staff_id, date, compliments, complaints, note }]);
          };
          return (
            <li key={s.staff_id} className="card grid gap-2 p-3 sm:grid-cols-[minmax(10rem,1fr)_auto_auto] sm:items-center">
              <p className="font-semibold">{s.full_name}</p>
              <Counter
                label="Khen"
                value={c}
                disabled={closed || save.isPending}
                onChange={(v) => {
                  put(v, k);
                }}
              />
              <Counter
                label="Phàn nàn"
                value={k}
                disabled={closed || save.isPending}
                onChange={(v) => {
                  put(c, v);
                }}
              />
              {r?.note && <p className="text-sm text-text-muted sm:col-span-3">{r.note}</p>}
            </li>
          );
        })}
      </ul>
      {error && (
        <p role="alert" className="text-sm text-danger">
          {error}
        </p>
      )}
    </>
  );
}

function Counter({ label, value, disabled, onChange }: { label: string; value: number; disabled: boolean; onChange: (v: number) => void }) {
  return (
    <div className="counter" role="group" aria-label={label}>
      <span className="counter-label">{label}</span>
      <button
        type="button"
        className="icon-btn"
        aria-label={`Bớt ${label.toLowerCase()}`}
        disabled={disabled || value === 0}
        onClick={() => {
          onChange(value - 1);
        }}
      >
        −
      </button>
      <span className="counter-value tabular" aria-live="polite">
        {value}
      </span>
      <button
        type="button"
        className="icon-btn"
        aria-label={`Thêm ${label.toLowerCase()}`}
        disabled={disabled || value >= 99}
        onClick={() => {
          onChange(value + 1);
        }}
      >
        +
      </button>
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
        <button type="submit" form={formId} className="btn btn-primary" disabled={save.isPending || (needsPlan && !plan.trim())}>
          Lưu đánh giá
        </button>
      }
    >
      <form
        id={formId}
        className="grid gap-4"
        onSubmit={(e) => {
          e.preventDefault();
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
