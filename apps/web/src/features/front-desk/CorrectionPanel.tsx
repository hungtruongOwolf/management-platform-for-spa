// The "Sửa lượt" part of the cart: what saving will do (direct edit or void + record), the money
// difference before saving, and the reason when money or sessions change. Lazy (own chunk).
import type { Visit } from "@vpyd/api-client";
import { formatVnd } from "@vpyd/ui";
import { type CorrectionDraft, planCorrection } from "./correction";

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
      <p
        role="status"
        className="note"
      >
        Chưa có gì thay đổi so với lượt đã ghi.
      </p>
    );
  if (plan.kind === "direct")
    return (
      <p role="status" className="note text-text">
        Chỉ đổi KTV, giờ làm hoặc ghi chú: lưu thẳng vào lượt này,{" "}
        <strong>không đổi tiền</strong> và buổi liệu trình.
      </p>
    );
  return (
    <div className="callout">
      <p>
        Đổi dịch vụ, tiền hoặc khách: lượt cũ được hủy (buổi cộng lại, tiền ghi
        bút toán đảo) và ghi lại lượt mới. Quầy chỉ thu hoặc hoàn phần chênh
        lệch ở ô thanh toán bên dưới.
      </p>
      <label className="field">
        <span>Lý do sửa (bắt buộc)</span>
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
          className="input"
        />
        {invalid && (
          <span id="correction-reason-error" className="field-error">
            Cần ghi lý do sửa
          </span>
        )}
      </label>
    </div>
  );
}

/**
 * The checkout amount while correcting: what the desk collects or gives back, big; the corrected
 * total only as context (EM live test: "Phải trả 349.000đ" next to "Thu thêm 50.000đ" read as
 * "collect 349k").
 */
export function CorrectionTotal({
  total,
  paid,
}: {
  /** The corrected visit's total (preview). */
  total: number;
  /** What the recorded visit took. */
  paid: number;
}) {
  const difference = total - paid;
  return (
    <>
      <p
        className="checkout-total"
        aria-live="polite"
        data-testid="correction-difference"
      >
        <span>
          {difference > 0
            ? "Thu thêm"
            : difference < 0
              ? "Hoàn lại khách"
              : "Không đổi tiền"}
        </span>
        <span className="checkout-amount">
          {formatVnd(Math.abs(difference))}
        </span>
      </p>
      <p className="checkout-split">
        Tổng lượt sau sửa {formatVnd(total)} · đã thu {formatVnd(paid)}
      </p>
    </>
  );
}
