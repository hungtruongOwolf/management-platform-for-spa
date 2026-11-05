// Cart options shown once a visit has lines: service time (giờ làm, owner back-dating) and the
// one-off discount. A lazy chunk prefetched after the first paint, off the startup bundle.
import { formatVnd } from "@vpyd/ui";
import { useState } from "react";
import { DateField } from "../../components/DateField";
import { type DiscountDraft, discountPreview } from "./api";

const field = "min-h-11 rounded-[6px] border border-border bg-surface px-3";

/** ISO day `days` before `day`. */
function daysBefore(day: string, days: number): string {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() - days);
  return d.toISOString().slice(0, 10);
}

/** "Giờ làm": now by default; the desk may pick an earlier time today, the owner an earlier day. */
export function ServiceTime({
  mayBackdate,
  today,
  day,
  time,
  invalid,
  onDay,
  onTime,
}: {
  mayBackdate: boolean;
  today: string;
  day: string;
  time: string;
  invalid: boolean;
  onDay: (iso: string) => void;
  onTime: (hm: string) => void;
}) {
  const [open, setOpen] = useState(day !== "" || time !== "");
  if (!open)
    return (
      <p className="flex flex-wrap items-center gap-2 text-sm">
        <span>
          Giờ làm: <strong>bây giờ</strong>
        </span>
        <button
          type="button"
          className="underline"
          onClick={() => {
            setOpen(true);
          }}
        >
          Đổi giờ làm{mayBackdate ? " / nhập bù ngày cũ" : ""}
        </button>
      </p>
    );
  return (
    <fieldset className="grid gap-2 rounded-[10px] border border-border p-3">
      <legend className="px-1 text-sm font-semibold">Giờ làm dịch vụ</legend>
      <div className="flex flex-wrap items-end gap-3">
        {mayBackdate && (
          <DateField
            label="Ngày làm"
            value={day || today}
            min={daysBefore(today, 366)}
            max={today}
            onChange={(iso) => {
              onDay(iso || today);
            }}
          />
        )}
        <label className="grid gap-1 text-sm">
          <span>Giờ (để trống = bây giờ)</span>
          <input
            type="time"
            step={300}
            value={time}
            aria-invalid={invalid}
            onChange={(e) => {
              onTime(e.target.value);
            }}
            className={`${field} ${invalid ? "border-danger" : ""}`}
          />
        </label>
      </div>
      {day !== "" && day < today && (
        <p className="text-sm text-warning-text">
          Nhập bù: lượt, hóa đơn và tiền thu được ghi vào ngày{" "}
          {day.split("-").reverse().join("/")}.
        </p>
      )}
    </fieldset>
  );
}

/** Discount on the one-off lines: % or amount + reason (D10). The server computes and checks. */
export function DiscountField({
  due,
  draft,
  overCap,
  onChange,
}: {
  due: number;
  draft: DiscountDraft | null;
  overCap: boolean;
  onChange: (d: DiscountDraft | null) => void;
}) {
  if (!draft)
    return (
      <button
        type="button"
        className="justify-self-start text-sm underline"
        onClick={() => {
          onChange({ kind: "percent", value: "", reason: "" });
        }}
      >
        + Giảm giá dịch vụ lẻ
      </button>
    );
  const amount = discountPreview(due, draft);
  return (
    <fieldset className="grid gap-2 rounded-[10px] border border-border p-3">
      <legend className="px-1 text-sm font-semibold">
        Giảm giá dịch vụ lẻ
      </legend>
      <div className="flex flex-wrap items-end gap-2">
        <div role="group" aria-label="Kiểu giảm" className="segmented">
          {(
            [
              ["percent", "%"],
              ["amount", "Số tiền"],
            ] as const
          ).map(([k, label]) => (
            <button
              key={k}
              type="button"
              aria-pressed={draft.kind === k}
              onClick={() => {
                onChange({ ...draft, kind: k, value: "" });
              }}
            >
              {label}
            </button>
          ))}
        </div>
        <label className="grid gap-1 text-sm">
          <span>
            {draft.kind === "percent" ? "Mức giảm (%)" : "Số tiền giảm (đ)"}
          </span>
          <input
            inputMode="numeric"
            value={draft.value}
            onChange={(e) => {
              onChange({ ...draft, value: e.target.value });
            }}
            className={`${field} w-32`}
          />
        </label>
        <label className="grid w-full min-w-0 basis-full gap-1 text-sm">
          <span>Lý do (bắt buộc)</span>
          <input
            value={draft.reason}
            maxLength={500}
            onChange={(e) => {
              onChange({ ...draft, reason: e.target.value });
            }}
            className={`${field} w-full`}
          />
        </label>
        <button
          type="button"
          className="min-h-11 rounded-[10px] border border-border px-3 text-sm"
          onClick={() => {
            onChange(null);
          }}
        >
          Bỏ giảm giá
        </button>
      </div>
      <p className="text-sm" aria-live="polite">
        {amount > 0 ? `Giảm ${formatVnd(amount)}` : "Nhập mức giảm hợp lệ."}
        {overCap && (
          <span className="block text-danger">
            Vượt mức quản lý được giảm: cần chủ spa.
          </span>
        )}
      </p>
    </fieldset>
  );
}
