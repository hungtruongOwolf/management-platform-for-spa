// The "Sửa lượt" part of the cart: what saving will do (direct edit or void + record), the money
// difference before saving, and the reason when money or sessions change. Lazy (own chunk).
import type { Visit } from "@vpyd/api-client";
import { type CorrectionDraft, differenceText, planCorrection } from "./correction";

export function CorrectionPanel({
  visit,
  oldNote,
  draft,
  invalid,
  onReason,
}: {
  visit: Visit;
  oldNote: string;
  draft: CorrectionDraft;
  /** Save was attempted without a reason. */
  invalid: boolean;
  onReason: (reason: string) => void;
}) {
  const plan = planCorrection(visit, oldNote, draft);
  if (plan.kind === "none")
    return (
      <p role="status" className="rounded-[10px] bg-surface-2 p-2 text-sm text-text-muted">
        Chưa có gì thay đổi so với lượt đã ghi.
      </p>
    );
  if (plan.kind === "direct")
    return (
      <p role="status" className="rounded-[10px] bg-surface-2 p-2 text-sm">
        Chỉ đổi KTV, giờ làm hoặc ghi chú: lưu thẳng vào lượt này,{" "}
        <strong>không đổi tiền</strong> và buổi liệu trình.
      </p>
    );
  return (
    <div className="grid gap-2 rounded-[10px] border border-warning-border bg-warning-bg p-3 text-sm text-warning-text">
      <p>
        Đổi dịch vụ, tiền hoặc khách: lượt cũ được hủy (buổi cộng lại, tiền ghi bút toán đảo) và
        ghi lại lượt mới.
      </p>
      <p className="text-base font-bold" aria-live="polite" data-testid="correction-difference">
        {differenceText(plan.difference)}
      </p>
      <label className="grid gap-1">
        <span className="font-semibold">Lý do sửa (bắt buộc)</span>
        <input
          value={draft.reason}
          maxLength={480}
          aria-required="true"
          aria-invalid={invalid}
          aria-describedby={invalid ? "correction-reason-error" : undefined}
          onChange={(e) => {
            onReason(e.target.value);
          }}
          placeholder="Ví dụ: chọn nhầm dịch vụ"
          className={`min-h-11 rounded-[6px] border bg-surface px-3 ${invalid ? "border-danger" : "border-border"}`}
        />
        {invalid && (
          <span id="correction-reason-error" className="text-danger">
            Cần ghi lý do sửa
          </span>
        )}
      </label>
    </div>
  );
}
