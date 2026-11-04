import type { Customer } from "@vpyd/api-client";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useForm } from "react-hook-form";
import { z } from "zod";
import { ApiError } from "../../lib/api";
import { fieldMessage, messageFor } from "../../lib/errors";
import { createCustomer, updateCustomer } from "./api";

const schema = z.object({
  full_name: z.string().trim().min(1, "Nhập họ tên."),
  phone: z
    .string()
    .trim()
    .refine(
      (v) => /^(\+?84|0)[35789]\d{8}$/.test(v.replace(/[\s.()-]/g, "")),
      "Số điện thoại chưa đúng (10 số, bắt đầu bằng 0).",
    ),
  gender: z.enum(["", "female", "male"]),
  birth_year: z
    .string()
    .trim()
    .refine(
      (v) =>
        v === "" ||
        (/^\d{4}$/.test(v) && Number(v) >= 1900 && Number(v) <= 2100),
      "Năm sinh 4 chữ số.",
    ),
  address: z.string(),
  note: z.string(),
  health_note: z.string(),
});
type Values = z.infer<typeof schema>;

/** Prefill from what the receptionist typed in search: digits → phone, otherwise → name. */
export function prefillFrom(
  typed: string,
): Pick<Values, "full_name" | "phone"> {
  const t = typed.trim();
  return /^[\d\s.+()-]+$/.test(t)
    ? { full_name: "", phone: t }
    : { full_name: t, phone: "" };
}

const input =
  "min-h-11 w-full rounded-[6px] border border-border bg-surface px-3";
const opt = (v: string) => (v.trim() === "" ? null : v.trim());

/**
 * Create (no `customer`) or edit a profile. The health note is only part of *creation*; once a
 * customer exists it is edited separately (HealthNoteEditor) because it has its own permission.
 */
export function CustomerForm({
  customer,
  typed = "",
  canWriteHealth,
  onSaved,
  onOpenExisting,
  onCancel,
}: {
  customer?: Customer;
  typed?: string;
  canWriteHealth: boolean;
  onSaved: (c: Customer) => void;
  onOpenExisting?: (id: string) => void;
  onCancel: () => void;
}) {
  const queryClient = useQueryClient();
  const form = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: {
      ...(customer
        ? { full_name: customer.full_name, phone: customer.phone }
        : prefillFrom(typed)),
      gender: customer?.gender ?? "",
      birth_year:
        customer?.birth_year != null ? String(customer.birth_year) : "",
      address: customer?.address ?? "",
      note: customer?.note ?? "",
      health_note: "",
    },
  });
  const mutation = useMutation({
    mutationFn: (v: Values) => {
      const profile = {
        full_name: v.full_name,
        phone: v.phone,
        gender: v.gender === "" ? null : v.gender,
        birth_year: v.birth_year === "" ? null : Number(v.birth_year),
        address: opt(v.address),
        source: customer?.source ?? null,
        note: opt(v.note),
      };
      return customer
        ? updateCustomer(customer.id, customer.version, profile)
        : createCustomer({
            ...profile,
            health_note: canWriteHealth ? opt(v.health_note) : null,
          });
    },
    onSuccess: async (saved) => {
      await queryClient.invalidateQueries({ queryKey: ["customers"] });
      onSaved(saved);
    },
  });
  const { errors } = form.formState;
  const existingId =
    mutation.error instanceof ApiError &&
    mutation.error.code === "customer.phone_taken"
      ? mutation.error.existingId
      : undefined;

  return (
    <form
      aria-label={customer ? "Sửa hồ sơ khách" : "Khách mới"}
      noValidate
      onSubmit={(e) =>
        void form.handleSubmit((v) => {
          mutation.mutate(v);
        })(e)
      }
      className="grid gap-3 rounded-[10px] border border-dashed border-border p-3 sm:grid-cols-2"
    >
      <label className="block">
        <span className="text-sm font-semibold">Họ tên</span>
        <input className={input} {...form.register("full_name")} />
        <span className="text-sm text-danger">
          {errors.full_name?.message ??
            fieldMessage(mutation.error, "full_name")}
        </span>
      </label>
      <label className="block">
        <span className="text-sm font-semibold">Số điện thoại</span>
        <input
          className={input}
          type="tel"
          inputMode="tel"
          {...form.register("phone")}
        />
        <span className="text-sm text-danger">
          {errors.phone?.message ?? fieldMessage(mutation.error, "phone")}
        </span>
      </label>
      <fieldset>
        <legend className="text-sm font-semibold">
          Giới tính (để tính giá)
        </legend>
        {(["female", "male", ""] as const).map((g) => (
          <label
            key={g}
            className="mr-4 inline-flex min-h-11 items-center gap-2"
          >
            <input type="radio" value={g} {...form.register("gender")} />{" "}
            {g === "female" ? "Nữ" : g === "male" ? "Nam" : "Chưa rõ"}
          </label>
        ))}
      </fieldset>
      <label className="block">
        <span className="text-sm font-semibold">Năm sinh</span>
        <input
          className={input}
          inputMode="numeric"
          {...form.register("birth_year")}
        />
        <span className="text-sm text-danger">
          {errors.birth_year?.message}
        </span>
      </label>
      <label className="block sm:col-span-2">
        <span className="text-sm font-semibold">Địa chỉ</span>
        <input className={input} {...form.register("address")} />
      </label>
      <label className="block sm:col-span-2">
        <span className="text-sm font-semibold">Ghi chú chung</span>
        <input className={input} {...form.register("note")} />
      </label>
      {!customer && canWriteHealth && (
        <label className="block sm:col-span-2">
          <span className="text-sm font-semibold">
            Lưu ý sức khỏe (chống chỉ định, tình trạng)
          </span>
          <textarea
            className={`${input} min-h-20 py-2`}
            {...form.register("health_note")}
          />
        </label>
      )}
      {mutation.isError && (
        <p
          role="alert"
          className="rounded-[10px] bg-danger-soft p-3 text-sm text-danger sm:col-span-2"
        >
          {existingId ? (
            <>
              Số điện thoại này đã có hồ sơ.{" "}
              <button
                type="button"
                className="font-semibold underline"
                onClick={() => {
                  onOpenExisting?.(existingId);
                }}
              >
                Mở hồ sơ đó
              </button>
            </>
          ) : (
            messageFor(mutation.error)
          )}
        </p>
      )}
      <div className="flex gap-2 sm:col-span-2">
        <button
          type="submit"
          disabled={mutation.isPending}
          className="min-h-11 rounded-[10px] bg-primary px-4 font-semibold text-on-primary disabled:opacity-60"
        >
          {mutation.isPending
            ? "Đang lưu…"
            : customer
              ? "Lưu thay đổi"
              : "Lưu khách"}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="min-h-11 rounded-[10px] border border-border px-4"
        >
          Hủy
        </button>
      </div>
    </form>
  );
}
