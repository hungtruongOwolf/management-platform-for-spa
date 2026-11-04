import type { Service, ServiceCategory, ServiceWrite } from "@vpyd/api-client";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useForm, useWatch } from "react-hook-form";
import { z } from "zod";
import { fieldMessage, messageFor } from "../../lib/errors";
import { saveService } from "./api";

const amount = z
  .string()
  .trim()
  .refine(
    (v) => v === "" || /^\d+$/.test(v.replace(/\./g, "")),
    "Nhập số tiền (chỉ chữ số).",
  );

const schema = z
  .object({
    name: z.string().trim().min(1, "Nhập tên dịch vụ."),
    category_id: z.string().min(1, "Chọn nhóm."),
    description: z.string(),
    duration: z
      .string()
      .trim()
      .refine(
        (v) =>
          v === "" || (/^\d+$/.test(v) && Number(v) >= 1 && Number(v) <= 600),
        "1–600 phút, hoặc để trống.",
      ),
    mode: z.enum(["all", "gender"]),
    all: amount,
    female: amount,
    male: amount,
    recommended: z.boolean(),
    active: z.boolean(),
  })
  .superRefine((v, ctx) => {
    if (!v.active) return;
    if (v.mode === "all" && v.all === "")
      ctx.addIssue({
        code: "custom",
        path: ["all"],
        message: "Nhập giá để bán dịch vụ.",
      });
    if (v.mode === "gender" && v.female === "" && v.male === "") {
      ctx.addIssue({
        code: "custom",
        path: ["female"],
        message: "Nhập ít nhất một giá (Nữ hoặc Nam).",
      });
    }
  });
type Values = z.infer<typeof schema>;

const toNumber = (v: string): number | null =>
  v === "" ? null : Number(v.replace(/\./g, ""));

export function toPayload(v: Values): ServiceWrite {
  return {
    category_id: v.category_id,
    name: v.name.trim(),
    description: v.description.trim() || null,
    duration_min: toNumber(v.duration),
    recommended: v.recommended,
    active: v.active,
    prices:
      v.mode === "all"
        ? { all: toNumber(v.all) }
        : { female: toNumber(v.female), male: toNumber(v.male) },
  };
}

function initialValues(
  service: Service | undefined,
  categoryId: string,
): Values {
  const p = service?.prices;
  const gendered =
    p != null && p.all == null && (p.female != null || p.male != null);
  return {
    name: service?.name ?? "",
    category_id: service?.category_id ?? categoryId,
    description: service?.description ?? "",
    duration: service?.duration_min != null ? String(service.duration_min) : "",
    mode: gendered ? "gender" : "all",
    all: p?.all != null ? String(p.all) : "",
    female: p?.female != null ? String(p.female) : "",
    male: p?.male != null ? String(p.male) : "",
    recommended: service?.recommended ?? false,
    active: service?.active ?? true,
  };
}

const input =
  "min-h-11 w-full rounded-[6px] border border-border bg-surface px-3";

/** Create or edit one service (owner, catalog.manage). Server re-validates everything. */
export function ServiceForm({
  categories,
  service,
  defaultCategoryId,
  onDone,
}: {
  categories: ServiceCategory[];
  service?: Service;
  defaultCategoryId: string;
  onDone: () => void;
}) {
  const queryClient = useQueryClient();
  const form = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: initialValues(service, defaultCategoryId),
  });
  const mutation = useMutation({
    mutationFn: (v: Values) => saveService(toPayload(v), service),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["catalog"] });
      onDone();
    },
  });
  const { errors } = form.formState;
  const mode = useWatch({ control: form.control, name: "mode" });
  const serverField = (field: string) => fieldMessage(mutation.error, field);
  const title = service ? `Sửa: ${service.name}` : "Thêm dịch vụ";

  return (
    <form
      aria-label={title}
      noValidate
      onSubmit={(e) =>
        void form.handleSubmit((v) => {
          mutation.mutate(v);
        })(e)
      }
      className="mt-2 grid gap-3 rounded-[10px] border border-primary bg-primary-soft/40 p-3 sm:grid-cols-2"
    >
      <h3 className="font-semibold sm:col-span-2">{title}</h3>
      <label className="block">
        <span className="text-sm font-semibold">Tên dịch vụ</span>
        <input
          className={input}
          {...form.register("name")}
          aria-invalid={errors.name || serverField("name") ? true : undefined}
        />
        <span className="text-sm text-danger">
          {errors.name?.message ?? serverField("name")}
        </span>
      </label>
      <label className="block">
        <span className="text-sm font-semibold">Nhóm</span>
        <select className={input} {...form.register("category_id")}>
          {categories.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </select>
      </label>
      <label className="block sm:col-span-2">
        <span className="text-sm font-semibold">Mô tả / phần kèm theo</span>
        <input className={input} {...form.register("description")} />
      </label>
      <label className="block">
        <span className="text-sm font-semibold">Thời lượng (phút)</span>
        <input
          className={input}
          inputMode="numeric"
          placeholder="Để trống = theo tư vấn"
          {...form.register("duration")}
        />
        <span className="text-sm text-danger">
          {errors.duration?.message ?? serverField("duration_min")}
        </span>
      </label>
      <fieldset className="block">
        <legend className="text-sm font-semibold">Cách tính giá</legend>
        <label className="mr-4 inline-flex min-h-11 items-center gap-2">
          <input type="radio" value="all" {...form.register("mode")} /> Một giá
          cho mọi khách
        </label>
        <label className="inline-flex min-h-11 items-center gap-2">
          <input type="radio" value="gender" {...form.register("mode")} /> Theo
          giới tính
        </label>
      </fieldset>
      {mode === "all" ? (
        <label className="block">
          <span className="text-sm font-semibold">Giá (đ)</span>
          <input
            className={input}
            inputMode="numeric"
            {...form.register("all")}
          />
          <span className="text-sm text-danger">
            {errors.all?.message ?? serverField("prices")}
          </span>
        </label>
      ) : (
        <>
          <label className="block">
            <span className="text-sm font-semibold">Giá Nữ (đ)</span>
            <input
              className={input}
              inputMode="numeric"
              placeholder="Để trống = không áp dụng"
              {...form.register("female")}
            />
            <span className="text-sm text-danger">
              {errors.female?.message ?? serverField("prices")}
            </span>
          </label>
          <label className="block">
            <span className="text-sm font-semibold">Giá Nam (đ)</span>
            <input
              className={input}
              inputMode="numeric"
              placeholder="Để trống = không áp dụng"
              {...form.register("male")}
            />
            <span className="text-sm text-danger">{errors.male?.message}</span>
          </label>
        </>
      )}
      <div className="flex flex-wrap gap-4 sm:col-span-2">
        <label className="inline-flex min-h-11 items-center gap-2">
          <input type="checkbox" {...form.register("recommended")} /> ★ Khuyên
          dùng
        </label>
        <label className="inline-flex min-h-11 items-center gap-2">
          <input type="checkbox" {...form.register("active")} /> Đang bán
        </label>
      </div>
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
          {mutation.isPending ? "Đang lưu…" : "Lưu"}
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
