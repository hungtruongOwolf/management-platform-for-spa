import type { PackagePlan, PlanWrite, Service } from "@vpyd/api-client";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useFieldArray, useForm } from "react-hook-form";
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

const input =
  "min-h-11 w-full rounded-[6px] border border-border bg-surface px-3";

/** Create or edit a package plan: one or more `service × sessions (+ bonus)` lines (D1, D3). */
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
  const title = plan ? `Sửa gói: ${plan.name}` : "Tạo gói liệu trình";

  return (
    <form
      aria-label={title}
      noValidate
      onSubmit={(e) =>
        void form.handleSubmit((v) => {
          mutation.mutate(v);
        })(e)
      }
      className="grid gap-3 rounded-[10px] border border-primary bg-primary-soft/40 p-3 sm:grid-cols-2"
    >
      <h3 className="font-semibold sm:col-span-2">{title}</h3>
      <label className="block">
        <span className="text-sm font-semibold">Tên gói</span>
        <input className={input} {...form.register("name")} />
        <span className="text-sm text-danger">
          {errors.name?.message ?? fieldMessage(mutation.error, "name")}
        </span>
      </label>
      <label className="block">
        <span className="text-sm font-semibold">Giá gói (đ)</span>
        <input
          className={input}
          inputMode="numeric"
          {...form.register("price")}
        />
        <span className="text-sm text-danger">
          {errors.price?.message ?? fieldMessage(mutation.error, "price")}
        </span>
      </label>
      <label className="block">
        <span className="text-sm font-semibold">Hạn dùng (tháng)</span>
        <input
          className={input}
          inputMode="numeric"
          placeholder="Để trống = không thời hạn"
          {...form.register("validity")}
        />
        <span className="text-sm text-danger">{errors.validity?.message}</span>
      </label>
      <label className="block">
        <span className="text-sm font-semibold">Quà tặng hiện vật</span>
        <input
          className={input}
          placeholder="Ví dụ: 1 lọ dầu gội thảo dược"
          {...form.register("gift_note")}
        />
      </label>

      <fieldset className="space-y-2 sm:col-span-2">
        <legend className="text-sm font-semibold">Dịch vụ trong gói</legend>
        {items.fields.map((field, index) => (
          <div
            key={field.id}
            className="grid grid-cols-[minmax(0,1fr)_6rem_6rem_auto] items-end gap-2"
          >
            <label className="block">
              <span className="sr-only">Dịch vụ {index + 1}</span>
              <select
                className={input}
                aria-label={`Dịch vụ ${String(index + 1)}`}
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
            <label className="block">
              <span className="text-xs text-text-muted">Số buổi</span>
              <input
                className={input}
                inputMode="numeric"
                aria-label={`Số buổi ${String(index + 1)}`}
                {...form.register(`items.${index}.sessions`)}
              />
            </label>
            <label className="block">
              <span className="text-xs text-text-muted">Buổi tặng</span>
              <input
                className={input}
                inputMode="numeric"
                aria-label={`Buổi tặng ${String(index + 1)}`}
                {...form.register(`items.${index}.bonus`)}
              />
            </label>
            <button
              type="button"
              onClick={() => {
                items.remove(index);
              }}
              className="min-h-11 rounded-[10px] border border-border px-3"
              aria-label={`Bỏ dòng ${String(index + 1)}`}
            >
              Bỏ
            </button>
            <span className="col-span-4 text-sm text-danger">
              {errors.items?.[index]?.service_id?.message ??
                errors.items?.[index]?.sessions?.message ??
                errors.items?.[index]?.bonus?.message ??
                fieldMessage(
                  mutation.error,
                  `items[${String(index)}].service_id`,
                )}
            </span>
          </div>
        ))}
        <span className="text-sm text-danger">
          {errors.items?.root?.message ??
            errors.items?.message ??
            fieldMessage(mutation.error, "items")}
        </span>
        <button
          type="button"
          onClick={() => {
            items.append({ service_id: "", sessions: "10", bonus: "0" });
          }}
          className="min-h-11 rounded-[10px] border border-border px-3"
        >
          + Thêm dịch vụ vào gói
        </button>
      </fieldset>

      <label className="inline-flex min-h-11 items-center gap-2 sm:col-span-2">
        <input type="checkbox" {...form.register("active")} /> Đang bán
      </label>
      {mutation.isError && (
        <p
          role="alert"
          className="rounded-[10px] bg-danger-soft p-3 text-sm text-danger sm:col-span-2"
        >
          {messageFor(mutation.error)}
        </p>
      )}
      <div className="flex gap-2 sm:col-span-2">
        <button
          type="submit"
          disabled={mutation.isPending}
          className="min-h-11 rounded-[10px] bg-primary px-4 font-semibold text-on-primary disabled:opacity-60"
        >
          {mutation.isPending ? "Đang lưu…" : "Lưu gói"}
        </button>
        <button
          type="button"
          onClick={onDone}
          className="min-h-11 rounded-[10px] border border-border px-4"
        >
          Hủy
        </button>
      </div>
    </form>
  );
}
