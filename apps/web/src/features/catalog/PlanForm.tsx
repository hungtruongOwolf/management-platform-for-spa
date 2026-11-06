import type { PackagePlan, PlanWrite, Service } from "@vpyd/api-client";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId } from "react";
import { useFieldArray, useForm } from "react-hook-form";
import { Icon } from "../../app/Icon";
import { z } from "zod";
import { fieldMessage, messageFor } from "../../lib/errors";
import { savePlan } from "./api";

const count = (min: number, message: string) =>
  z
    .string()
    .trim()
    .refine(
      (v) => /^\d+$/.test(v) && Number(v) >= min && Number(v) <= 500,
      message,
    );

const schema = z.object({
  name: z.string().trim().min(1, "Nhập tên gói."),
  price: z
    .string()
    .trim()
    .refine(
      (v) => /^\d+$/.test(v.replace(/\./g, "")),
      "Nhập giá gói (chỉ chữ số).",
    ),
  validity: z
    .string()
    .trim()
    .refine(
      (v) =>
        v === "" || (/^\d+$/.test(v) && Number(v) >= 1 && Number(v) <= 120),
      "1–120 tháng, hoặc để trống.",
    ),
  gift_note: z.string(),
  active: z.boolean(),
  items: z
    .array(
      z.object({
        service_id: z.string().min(1, "Chọn dịch vụ."),
        sessions: count(1, "Số buổi từ 1 đến 500."),
        bonus: count(0, "Buổi tặng từ 0 đến 500."),
      }),
    )
    .min(1, "Gói cần ít nhất một dịch vụ."),
});
type Values = z.infer<typeof schema>;

export function toPlanPayload(v: Values): PlanWrite {
  return {
    name: v.name.trim(),
    price: Number(v.price.replace(/\./g, "")),
    validity_months: v.validity === "" ? null : Number(v.validity),
    gift_note: v.gift_note.trim() || null,
    active: v.active,
    items: v.items.map((i) => ({
      service_id: i.service_id,
      sessions: Number(i.sessions),
      bonus_sessions: Number(i.bonus),
    })),
  };
}

function initialValues(plan?: PackagePlan): Values {
  return {
    name: plan?.name ?? "",
    price: plan ? String(plan.price) : "",
    validity: plan?.validity_months != null ? String(plan.validity_months) : "",
    gift_note: plan?.gift_note ?? "",
    active: plan?.active ?? true,
    items: plan?.items.map((i) => ({
      service_id: i.service_id,
      sessions: String(i.sessions),
      bonus: String(i.bonus_sessions),
    })) ?? [{ service_id: "", sessions: "10", bonus: "0" }],
  };
}

/**
 * Create or edit a package plan: one or more `service × sessions (+ bonus)` lines (D1, D3).
 * Opens inline (under its row, or at the top of the page) with a hairline above: no box.
 */
export function PlanForm({
  services,
  plan,
  onDone,
}: {
  services: Service[];
  plan?: PackagePlan;
  onDone: () => void;
}) {
  const queryClient = useQueryClient();
  const id = useId();
  const form = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: initialValues(plan),
  });
  const items = useFieldArray({ control: form.control, name: "items" });
  const mutation = useMutation({
    mutationFn: (v: Values) => savePlan(toPlanPayload(v), plan),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["catalog"] });
      onDone();
    },
  });
  const { errors } = form.formState;
  const sellable = services.filter((s) => s.active);
  const title = plan ? `Sửa mẫu gói: ${plan.name}` : "Tạo mẫu gói liệu trình";
  const nameError = errors.name?.message ?? fieldMessage(mutation.error, "name");
  const priceError = errors.price?.message ?? fieldMessage(mutation.error, "price");
  const validityError = errors.validity?.message;
  const itemsError = errors.items?.root?.message ?? errors.items?.message ?? fieldMessage(mutation.error, "items");

  return (
    <form
      aria-label={title}
      noValidate
      onSubmit={(e) =>
        void form.handleSubmit((v) => {
          if (!mutation.isPending) mutation.mutate(v);
        })(e)
      }
      onKeyDown={(e) => {
        if (e.key === "Escape") onDone();
      }}
      className="row-form"
    >
      <h3 className="row-form-title">{title}</h3>
      <div className="form-grid">
        <div className="field">
          <label htmlFor={`${id}-name`}>Tên gói</label>
          <input
            id={`${id}-name`}
            className="input"
            autoComplete="off"
            placeholder="Ví dụ: Gội đầu 10 + tặng 2"
            aria-invalid={nameError ? true : undefined}
            aria-describedby={nameError ? `${id}-name-error` : undefined}
            {...form.register("name")}
          />
          {nameError && (
            <span id={`${id}-name-error`} className="field-error">
              {nameError}
            </span>
          )}
        </div>
        <div className="field">
          <label htmlFor={`${id}-price`}>Giá gói (đ)</label>
          <input
            id={`${id}-price`}
            className="input tabular"
            inputMode="numeric"
            aria-invalid={priceError ? true : undefined}
            aria-describedby={priceError ? `${id}-price-error` : undefined}
            {...form.register("price")}
          />
          {priceError && (
            <span id={`${id}-price-error`} className="field-error">
              {priceError}
            </span>
          )}
        </div>
        <div className="field">
          <label htmlFor={`${id}-validity`}>Hạn dùng (tháng)</label>
          <input
            id={`${id}-validity`}
            className="input tabular"
            inputMode="numeric"
            aria-invalid={validityError ? true : undefined}
            aria-describedby={[validityError ? `${id}-validity-error` : undefined, `${id}-validity-hint`].filter(Boolean).join(" ")}
            {...form.register("validity")}
          />
          <span id={`${id}-validity-hint`} className="field-hint">
            Để trống = không thời hạn.
          </span>
          {validityError && (
            <span id={`${id}-validity-error`} className="field-error">
              {validityError}
            </span>
          )}
        </div>
        <div className="field">
          <label htmlFor={`${id}-gift`}>Quà tặng hiện vật</label>
          <input id={`${id}-gift`} className="input" autoComplete="off" placeholder="Ví dụ: 1 lọ dầu gội thảo dược" {...form.register("gift_note")} />
        </div>

        <fieldset className="span-2">
          <legend>Dịch vụ trong gói</legend>
          <div className="grid gap-3">
            {items.fields.map((field, index) => {
              const n = String(index + 1);
              const lineError =
                errors.items?.[index]?.service_id?.message ??
                errors.items?.[index]?.sessions?.message ??
                errors.items?.[index]?.bonus?.message ??
                fieldMessage(mutation.error, `items[${String(index)}].service_id`);
              return (
                <div key={field.id} className="plan-line">
                  <label className="field" htmlFor={`${id}-svc-${n}`}>
                    <span aria-hidden="true">Dịch vụ</span>
                    <select
                      id={`${id}-svc-${n}`}
                      className="input"
                      aria-label={`Dịch vụ ${n}`}
                      aria-invalid={lineError ? true : undefined}
                      aria-describedby={lineError ? `${id}-line-${n}-error` : undefined}
                      {...form.register(`items.${index}.service_id`)}
                    >
                      <option value="">Chọn dịch vụ…</option>
                      {sellable.map((s) => (
                        <option key={s.id} value={s.id}>
                          {s.name}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="field" htmlFor={`${id}-sessions-${n}`}>
                    <span aria-hidden="true">Số buổi</span>
                    <input
                      id={`${id}-sessions-${n}`}
                      className="input tabular"
                      inputMode="numeric"
                      aria-label={`Số buổi ${n}`}
                      {...form.register(`items.${index}.sessions`)}
                    />
                  </label>
                  <label className="field" htmlFor={`${id}-bonus-${n}`}>
                    <span aria-hidden="true">Buổi tặng</span>
                    <input
                      id={`${id}-bonus-${n}`}
                      className="input tabular"
                      inputMode="numeric"
                      aria-label={`Buổi tặng ${n}`}
                      {...form.register(`items.${index}.bonus`)}
                    />
                  </label>
                  <button
                    type="button"
                    onClick={() => {
                      items.remove(index);
                    }}
                    className="btn btn-ghost btn-icon plan-line-remove"
                    aria-label={`Bỏ dịch vụ ${n} khỏi gói`}
                  >
                    <Icon name="x" className="size-4" />
                  </button>
                  {lineError && (
                    <span id={`${id}-line-${n}-error`} className="field-error plan-line-error">
                      {lineError}
                    </span>
                  )}
                </div>
              );
            })}
            {itemsError && <span className="field-error">{itemsError}</span>}
            <button
              type="button"
              onClick={() => {
                items.append({ service_id: "", sessions: "10", bonus: "0" });
              }}
              className="btn btn-secondary"
            >
              <Icon name="plus" className="size-4" /> Thêm dịch vụ vào gói
            </button>
          </div>
        </fieldset>

        <div className="choice-row span-2">
          <label className="choice">
            <input type="checkbox" {...form.register("active")} /> Đang bán
          </label>
        </div>
        {mutation.isError && (
          <p role="alert" className="form-error span-2">
            {messageFor(mutation.error)}
          </p>
        )}
        <div className="form-actions span-2">
          <button type="submit" aria-busy={mutation.isPending || undefined} className="btn btn-primary">
            Lưu mẫu gói
          </button>
          <button type="button" onClick={onDone} className="btn btn-secondary">
            Hủy
          </button>
        </div>
      </div>
    </form>
  );
}
