import type { Service, ServiceCategory, ServiceWrite } from "@vpyd/api-client";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId } from "react";
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

/** A field's error under it, linked by aria-describedby. */
function FieldError({ id, message }: { id: string; message: string | undefined }) {
  return message ? (
    <span id={id} className="field-error">
      {message}
    </span>
  ) : null;
}

/**
 * Create or edit one service (owner, catalog.manage), inline under its row or at the top of its
 * group: a hairline above, no box (no card in a card). Server re-validates everything.
 */
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
  const id = useId();
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
  const err = {
    name: errors.name?.message ?? serverField("name"),
    duration: errors.duration?.message ?? serverField("duration_min"),
    all: errors.all?.message ?? serverField("prices"),
    female: errors.female?.message ?? serverField("prices"),
    male: errors.male?.message,
  };
  const wire = (key: keyof typeof err, hint?: string) => ({
    "aria-invalid": err[key] ? true : undefined,
    "aria-describedby": [err[key] ? `${id}-${key}-error` : undefined, hint].filter(Boolean).join(" ") || undefined,
  });

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
          <label htmlFor={`${id}-name`}>Tên dịch vụ</label>
          <input id={`${id}-name`} className="input" autoComplete="off" {...wire("name")} {...form.register("name")} />
          <FieldError id={`${id}-name-error`} message={err.name} />
        </div>
        <div className="field">
          <label htmlFor={`${id}-category`}>Nhóm</label>
          <select id={`${id}-category`} className="input" {...form.register("category_id")}>
            {categories.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </div>
        <div className="field span-2">
          <label htmlFor={`${id}-description`}>Mô tả / phần kèm theo</label>
          <input id={`${id}-description`} className="input" autoComplete="off" placeholder="Ví dụ: + Cạo gió đầu, đá nóng" {...form.register("description")} />
        </div>
        <div className="field">
          <label htmlFor={`${id}-duration`}>Thời lượng (phút)</label>
          <input
            id={`${id}-duration`}
            className="input tabular"
            inputMode="numeric"
            {...wire("duration", `${id}-duration-hint`)}
            {...form.register("duration")}
          />
          <span id={`${id}-duration-hint`} className="field-hint">
            Để trống = theo tư vấn.
          </span>
          <FieldError id={`${id}-duration-error`} message={err.duration} />
        </div>
        <fieldset>
          <legend>Cách tính giá</legend>
          <div className="choice-row">
            <label className="choice">
              <input type="radio" value="all" {...form.register("mode")} /> Một giá cho mọi khách
            </label>
            <label className="choice">
              <input type="radio" value="gender" {...form.register("mode")} /> Theo giới tính
            </label>
          </div>
        </fieldset>
        {mode === "all" ? (
          <div className="field">
            <label htmlFor={`${id}-all`}>Giá (đ)</label>
            <input id={`${id}-all`} className="input tabular" inputMode="numeric" {...wire("all")} {...form.register("all")} />
            <FieldError id={`${id}-all-error`} message={err.all} />
          </div>
        ) : (
          <>
            <div className="field">
              <label htmlFor={`${id}-female`}>Giá Nữ (đ)</label>
              <input
                id={`${id}-female`}
                className="input tabular"
                inputMode="numeric"
                {...wire("female", `${id}-gender-hint`)}
                {...form.register("female")}
              />
              <span id={`${id}-gender-hint`} className="field-hint">
                Để trống = không áp dụng.
              </span>
              <FieldError id={`${id}-female-error`} message={err.female} />
            </div>
            <div className="field">
              <label htmlFor={`${id}-male`}>Giá Nam (đ)</label>
              <input id={`${id}-male`} className="input tabular" inputMode="numeric" {...wire("male", `${id}-gender-hint`)} {...form.register("male")} />
              <FieldError id={`${id}-male-error`} message={err.male} />
            </div>
          </>
        )}
        <div className="choice-row span-2">
          <label className="choice">
            <input type="checkbox" {...form.register("recommended")} /> ★ Khuyên dùng
          </label>
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
            {service ? "Lưu dịch vụ" : "Thêm dịch vụ vào bảng giá"}
          </button>
          <button type="button" onClick={onDone} className="btn btn-secondary">
            Hủy
          </button>
        </div>
      </div>
    </form>
  );
}
