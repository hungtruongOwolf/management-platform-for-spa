// The cart's secondary pieces: the one-off service picker, and once a visit has lines the service
// time (giờ làm, owner back-dating) and the one-off discount. Each is one aligned row — label · current value · one ghost action — that
// opens its fields underneath. A lazy chunk prefetched after the first paint, off the startup bundle.
import type { CustomerSummary, Service } from "@vpyd/api-client";
import { useQuery } from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import { useRef, useState } from "react";
import { DateField } from "../../components/DateField";
import { messageFor } from "../../lib/errors";
import { categoriesQuery, servicesQuery } from "../catalog/api";
import {
  type DiscountDraft,
  type Gender,
  discountPreview,
  priceFor,
} from "./api";

/** ISO day `days` before `day`. */
function daysBefore(day: string, days: number): string {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() - days);
  return d.toISOString().slice(0, 10);
}

const dmy = (iso: string) => iso.split("-").reverse().join("/");

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
  const backdated = day !== "" && day < today;
  const value =
    time === "" && !backdated
      ? "Bây giờ"
      : `${time || "Chưa chọn giờ"}${backdated ? ` · ${dmy(day)}` : " hôm nay"}`;
  return (
    <div className="option-row">
      <span className="option-label">Giờ làm</span>
      <span className="option-value">{value}</span>
      {open ? (
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => {
            onDay(today);
            onTime("");
            setOpen(false);
          }}
        >
          Dùng giờ hiện tại
        </button>
      ) : (
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => {
            setOpen(true);
          }}
        >
          {mayBackdate ? "Đổi giờ / nhập bù" : "Đổi giờ làm"}
        </button>
      )}
      {open && (
        <fieldset className="option-panel">
          <legend className="sr-only">Giờ làm dịch vụ</legend>
          <div className="option-fields">
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
            <label className="field">
              <span>Giờ (để trống = bây giờ)</span>
              <input
                type="time"
                step={300}
                value={time}
                aria-invalid={invalid}
                onChange={(e) => {
                  onTime(e.target.value);
                }}
                className="input w-40"
              />
            </label>
          </div>
          {backdated && (
            <p className="text-sm text-warning-text">
              Nhập bù: lượt, hóa đơn và tiền thu được ghi vào ngày {dmy(day)}.
            </p>
          )}
        </fieldset>
      )}
    </div>
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
  const amount = draft ? discountPreview(due, draft) : 0;
  return (
    <div className="option-row">
      <span className="option-label">Giảm giá</span>
      <span className="option-value" aria-live="polite">
        {!draft
          ? "Không"
          : amount > 0
            ? `Giảm ${formatVnd(amount)}`
            : "Nhập mức giảm hợp lệ."}
      </span>
      {draft ? (
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => {
            onChange(null);
          }}
        >
          Bỏ giảm giá
        </button>
      ) : (
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => {
            onChange({ kind: "percent", value: "", reason: "" });
          }}
        >
          Thêm giảm giá
        </button>
      )}
      {draft && (
        <fieldset className="option-panel">
          <legend className="sr-only">Giảm giá dịch vụ lẻ</legend>
          <div className="option-fields">
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
            <label className="field">
              <span>
                {draft.kind === "percent" ? "Mức giảm (%)" : "Số tiền giảm (đ)"}
              </span>
              <input
                inputMode="numeric"
                value={draft.value}
                onChange={(e) => {
                  onChange({ ...draft, value: e.target.value });
                }}
                className="input w-36"
              />
            </label>
          </div>
          <label className="field">
            <span>Lý do (bắt buộc)</span>
            <input
              value={draft.reason}
              maxLength={500}
              onChange={(e) => {
                onChange({ ...draft, reason: e.target.value });
              }}
              className="input"
            />
          </label>
          {overCap && (
            <p className="field-error">
              Vượt mức quản lý được giảm: cần chủ spa.
            </p>
          )}
        </fieldset>
      )}
    </div>
  );
}

/**
 * One-off service: grouped by category, ★ first; price by the customer's gender. Picking a
 * service adds it (no separate "Thêm": a pick that waited for it was lost, booking bug #44). A
 * closed select also changes value on ↑/↓ and type-ahead, so a keyboard pick waits for Enter.
 */
export function AddOneOff({
  customer,
  onAdd,
  onProblem,
}: {
  customer: CustomerSummary;
  onAdd: (
    service: Service,
    priceVariant: Gender | null,
    price: number,
    categoryName: string | undefined,
  ) => void;
  onProblem: (message: string) => void;
}) {
  const services = useQuery(servicesQuery(false));
  const categories = useQuery(categoriesQuery);
  const [pending, setPending] = useState("");
  const viaKeys = useRef(false);
  const [chosenGender, setChosenGender] = useState<Gender | null>(null);
  const known = customer.gender ?? null;
  const gender = known ?? chosenGender;

  const groups = (categories.data ?? []).map((c) => ({
    category: c,
    items: (services.data ?? [])
      .filter((s) => s.category_id === c.id)
      .sort((a, b) => Number(b.recommended) - Number(a.recommended)),
  }));

  const add = (serviceId: string) => {
    setPending("");
    const service = services.data?.find((s) => s.id === serviceId);
    if (!service) return;
    const r = priceFor(service, gender);
    if (!r.ok) {
      onProblem(
        r.reason === "need_gender"
          ? "Chọn Nữ hoặc Nam để tính giá dịch vụ này."
          : r.reason === "not_offered"
            ? `“${service.name}” không áp dụng cho khách ${gender === "male" ? "Nam" : "Nữ"}.`
            : `“${service.name}” chưa có giá.`,
      );
      return;
    }
    onAdd(
      service,
      known ? null : r.variant === "all" ? null : r.variant,
      r.price,
      categories.data?.find((c) => c.id === service.category_id)?.name,
    );
  };

  const priceLabel = (s: Service) =>
    s.prices.all != null
      ? formatVnd(s.prices.all)
      : `Nữ ${s.prices.female != null ? formatVnd(s.prices.female) : "—"} / Nam ${s.prices.male != null ? formatVnd(s.prices.male) : "—"}`;
  const pendingName = services.data?.find((s) => s.id === pending)?.name;

  return (
    <div className="grid gap-2">
      <label htmlFor="one-off-service" className="section-label">
        Thêm dịch vụ lẻ
      </label>
      {services.isError && (
        <p role="alert" className="field-error">
          {messageFor(services.error)}
        </p>
      )}
      <div className="flex flex-wrap items-center gap-2">
        {!known && (
          <span
            role="group"
            aria-label="Giới tính để tính giá"
            className="segmented"
          >
            {(
              [
                ["female", "Nữ"],
                ["male", "Nam"],
              ] as const
            ).map(([g, label]) => (
              <button
                key={g}
                type="button"
                aria-pressed={chosenGender === g}
                onClick={() => {
                  setChosenGender(g);
                }}
              >
                {label}
              </button>
            ))}
          </span>
        )}
        <select
          id="one-off-service"
          value={pending}
          aria-describedby={pending ? "one-off-pending" : undefined}
          onPointerDown={() => {
            viaKeys.current = false;
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              if (pending) {
                e.preventDefault();
                add(pending);
              }
              return;
            }
            // Keys that step through a closed select; Space / Alt+↓ / F4 open the list instead.
            viaKeys.current =
              (!e.altKey &&
                /^(ArrowUp|ArrowDown|Home|End|PageUp|PageDown)$/.test(e.key)) ||
              (e.key.length === 1 && e.key !== " ");
          }}
          onChange={(e) => {
            const id = e.target.value;
            if (viaKeys.current) setPending(id);
            else add(id);
          }}
          // A definite width: a select's intrinsic width is its longest option, which would
          // otherwise widen the whole column past the viewport.
          className="input w-56 min-w-0 flex-1"
        >
          <option value="">
            {services.isPending
              ? "Đang tải bảng giá…"
              : "Chọn dịch vụ để thêm…"}
          </option>
          {groups
            .filter((g) => g.items.length > 0)
            .map((g) => (
              <optgroup key={g.category.id} label={g.category.name}>
                {g.items.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.recommended ? "★ " : ""}
                    {s.name} · {priceLabel(s)}
                  </option>
                ))}
              </optgroup>
            ))}
        </select>
      </div>
      {pendingName ? (
        <p id="one-off-pending" className="field-hint">
          Nhấn Enter để thêm “{pendingName}”.
        </p>
      ) : (
        !known && (
          <p className="field-hint">
            Khách chưa có giới tính: chọn Nữ/Nam để tính giá.
          </p>
        )
      )}
    </div>
  );
}
