import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { messageFor } from "../../lib/errors";
import { ErrorState, RowsSkeleton } from "../reports/ui";
import { evaluationsQuery, invalidateStaff, saveEvaluation, type EvaluationSheet, type StaffEvaluation } from "./api";
import { EvaluationHistoryView, ResultChip, ScoreCard } from "./EvaluationViews";
import { LEVEL_LABEL, LEVEL_SHORT, monthLabel, type Level } from "./labels";
import { ClosedBanner } from "./ui";

/**
 * "Đánh giá tháng": the month's 100-point scorecard per person. Daily praise/complaints live in
 * the daily book; the book prompts for this step only while it is due (spec §10 AC7.4).
 * EM 2026-11-08: every scored row opens its scorecard read-only ("Xem"), closed months and rows
 * the viewer may not rescore included; a name opens that person's history across months
 * (`staff` in the URL). "Chấm điểm / Chấm lại" keeps its rule.
 */
export function EvaluationTab({
  month,
  staffId,
  onPerson,
}: {
  month: string;
  /** A person's history instead of the month list ("" = the month list). */
  staffId: string;
  onPerson: (id: string) => void;
}) {
  if (staffId) {
    return (
      <EvaluationHistoryView
        staffId={staffId}
        onBack={() => {
          onPerson("");
        }}
      />
    );
  }
  return <MonthList month={month} onPerson={onPerson} />;
}

function MonthList({ month, onPerson }: { month: string; onPerson: (id: string) => void }) {
  const evals = useQuery(evaluationsQuery(month));
  const [scoring, setScoring] = useState<StaffEvaluation | null>(null);
  const [viewing, setViewing] = useState<StaffEvaluation | null>(null);

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
              <button
                type="button"
                className="name-link font-semibold"
                aria-label={`${s.full_name}: lịch sử đánh giá các tháng`}
                onClick={() => {
                  onPerson(s.staff_id);
                }}
              >
                {s.full_name}
              </button>
              <p className="text-sm text-text-muted">
                {s.compliments} khen · {s.complaints} phàn nàn · nghỉ {s.attendance.leave_p + s.attendance.leave_dx + s.attendance.absent_k} ngày ·
                muộn {s.attendance.late_count} lần
              </p>
            </div>
            {s.evaluation ? <ResultChip evaluation={s.evaluation} /> : <span className="badge badge-warn">Chưa đánh giá</span>}
            {s.evaluation && (
              <button
                type="button"
                className="btn btn-ghost"
                aria-label={`Xem phiếu đánh giá ${s.full_name}`}
                onClick={() => {
                  setViewing(s);
                }}
              >
                Xem
              </button>
            )}
            {!sheet.closed && s.can_record && (
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
            {!sheet.closed && !s.can_record && <span className="text-sm text-text-muted">Chỉ chủ spa chấm cho quản lý</span>}
          </li>
        ))}
      </ul>
      {viewing?.evaluation && (
        <ScoreCard
          name={viewing.full_name}
          month={sheet.month}
          evaluation={viewing.evaluation}
          onClose={() => {
            setViewing(null);
          }}
          footer={
            !sheet.closed && viewing.can_record ? (
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => {
                  setScoring(viewing);
                  setViewing(null);
                }}
              >
                Chấm lại
              </button>
            ) : undefined
          }
        />
      )}
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

/** The sheet's guide: each level as a share of a criterion's maximum (Chưa đạt is "at most"). */
const ANCHORS: [Level, number][] = [
  ["m1", 100],
  ["m2", 80],
  ["m3", 60],
  ["fail", 30],
];

/** Level of a total out of 100, or of a criterion's share of its maximum in percent (as the API). */
export function levelOf(percent: number): Level {
  return percent >= 85 ? "m1" : percent >= 70 ? "m2" : percent >= 50 ? "m3" : "fail";
}

/** "Mức 1: 40 · Mức 2: 32 · Mức 3: 24 · Chưa đạt: ≤ 12" for a criterion of maximum 40. */
export function anchorsOf(max: number): string {
  return ANCHORS.map(([l, pct]) => `${LEVEL_SHORT[l]}: ${l === "fail" ? "≤ " : ""}${String(Math.floor((max * pct) / 100))}`).join(" · ");
}

/** A typed score: a whole number from 0 to `max`, or `null` while empty / invalid. */
export function parsePoints(text: string, max: number): number | null {
  if (!/^\d{1,3}$/.test(text.trim())) return null;
  const n = Number(text.trim());
  return n <= max ? n : null;
}

/**
 * The 100-point scorecard (EM decision D, 2026-11-07): the evaluator types each criterion's points;
 * a new form starts empty and saves only once every criterion has a valid score (the API checks the
 * same). The sheet's levels stay as a guide under each field, and a serious issue still caps the
 * month at Mức 3.
 */
function ScoreSheet({ sheet, staff, onClose }: { sheet: EvaluationSheet; staff: StaffEvaluation; onClose: () => void }) {
  const queryClient = useQueryClient();
  const initial = Object.fromEntries(
    sheet.criteria.map((c) => {
      const saved = staff.evaluation?.criteria.find((x) => x.key === c.key)?.points;
      return [c.key, saved == null ? "" : String(saved)];
    }),
  ) as Record<string, string>;
  const [texts, setTexts] = useState(initial);
  const [attempted, setAttempted] = useState(false);
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
  const points = sheet.criteria.map((c) => parsePoints(texts[c.key] ?? "", c.max));
  const missing = sheet.criteria.filter((_, i) => points[i] == null);
  const complete = missing.length === 0;
  const total = points.reduce<number>((sum, p) => sum + (p ?? 0), 0);
  const byTotal = levelOf(total);
  const result: Level = serious && (byTotal === "m1" || byTotal === "m2") ? "m3" : byTotal;
  const needsPlan = complete && (result === "m3" || result === "fail");
  const problem = !complete
    ? `Còn ${String(missing.length)} tiêu chí chưa nhập điểm: ${missing.map((c) => c.label).join(", ")}.`
    : needsPlan && !plan.trim()
      ? "Kết quả Mức 3 hoặc Chưa đạt: cần ghi kế hoạch kèm cặp tháng sau."
      : null;
  const formId = "score-form";
  return (
    <BottomSheet
      open
      wide
      onClose={onClose}
      title={`Đánh giá ${staff.full_name}`}
      description={complete ? `${String(total)} điểm · ${LEVEL_LABEL[result]}` : `Đang chấm: ${String(total)}/100 điểm`}
      footer={
        <button type="submit" form={formId} className="btn btn-primary" aria-busy={save.isPending || undefined} aria-describedby="score-problem">
          Lưu đánh giá
        </button>
      }
    >
      <form
        id={formId}
        className="grid gap-4"
        noValidate
        onSubmit={(e) => {
          e.preventDefault();
          setAttempted(true);
          if (save.isPending || problem) return;
          save.mutate({
            staff_id: staff.staff_id,
            month: sheet.month,
            criteria: sheet.criteria.map((c, i) => ({ key: c.key, points: points[i] ?? null })),
            serious_issue: serious,
            coaching_plan: plan.trim() || null,
            comment: comment.trim() || null,
          });
        }}
      >
        <div className="note text-sm">
          <p>
            Nhập điểm từng tiêu chí, từ 0 đến điểm tối đa. Thang của bảng đánh giá: Mức 1 (Xuất sắc) = 100% điểm tối đa, Mức 2 (Tốt) = 80%,
            Mức 3 (Đạt) = 60%, Chưa đạt ≤ 30%; điểm lẻ giữa các mức vẫn được.
          </p>
          <p>Xếp loại tháng theo tổng: từ 85 Mức 1 · 70–84 Mức 2 · 50–69 Mức 3 · dưới 50 Chưa đạt.</p>
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          {sheet.criteria.map((c, i) => {
            const text = texts[c.key] ?? "";
            const p = points[i] ?? null;
            const invalid = text.trim() !== "" && p == null;
            const hintId = `score-hint-${c.key}`;
            return (
              <label key={c.key} className="filter-field">
                {c.label} (tối đa {c.max})
                <input
                  inputMode="numeric"
                  value={text}
                  placeholder={`0–${String(c.max)}`}
                  aria-invalid={invalid || (attempted && p == null) ? true : undefined}
                  aria-describedby={hintId}
                  onChange={(e) => {
                    setTexts({ ...texts, [c.key]: e.target.value.replace(/\D/g, "").slice(0, 3) });
                  }}
                />
                <span id={hintId} className={`text-xs ${invalid ? "text-danger" : "text-text-muted"}`}>
                  {invalid
                    ? `Tối đa ${String(c.max)} điểm.`
                    : p != null
                      ? `${LEVEL_SHORT[levelOf(Math.floor((p * 100) / c.max))]} · ${anchorsOf(c.max)}`
                      : anchorsOf(c.max)}
                </span>
              </label>
            );
          })}
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
        {complete && serious && result !== byTotal && (
          <p className="text-sm text-text-muted">
            Tổng {total} điểm là {LEVEL_SHORT[byTotal]}, nhưng có khiếu nại nghiêm trọng nên kết quả tháng tối đa Mức 3.
          </p>
        )}
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
        <p id="score-problem" role={attempted && problem ? "alert" : undefined} className={`text-sm ${attempted && problem ? "text-danger" : "text-text-muted"}`}>
          {problem ?? ""}
        </p>
        {save.isError && (
          <p role="alert" className="text-sm text-danger">
            {messageFor(save.error)}
          </p>
        )}
      </form>
    </BottomSheet>
  );
}
