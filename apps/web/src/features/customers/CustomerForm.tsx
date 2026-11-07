import type { Customer } from "@vpyd/api-client";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useState } from "react";
import { Controller, useForm, useWatch } from "react-hook-form";
import { z } from "zod";
import { DateField } from "../../components/DateField";
import { ApiError } from "../../lib/api";
import { fieldMessage, messageFor } from "../../lib/errors";
import { spaToday } from "../corrections/api";
import "./khach.css";
import {
  PHONE_CHECK_DEBOUNCE_MS,
  birthDateBounds,
  createCustomer,
  phoneCheckQuery,
  phoneDigits,
  updateCustomer,
} from "./api";

const BIRTH_DATE_INVALID = "Ngày sinh dạng dd/mm/yyyy, không ở tương lai, không quá 120 năm trước.";

function schemaFor(today: string) {
  const { min, max } = birthDateBounds(today);
  return z
    .object({
      // Both optional (EM 2026-11-07): a walk-in may give neither. No name = "Khách vãng lai".
      full_name: z.string().trim(),
      phone: z
        .string()
        .trim()
        .refine(
          (v) => v === "" || /^(\+?84|0)[35789]\d{8}$/.test(v.replace(/[\s.()-]/g, "")),
          "Số điện thoại chưa đúng (10 số, bắt đầu bằng 0).",
        ),
      gender: z.enum(["", "female", "male"]),
      birth_date: z
        .string()
        .refine((v) => v === "" || (v >= min && v <= max), BIRTH_DATE_INVALID),
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
}
type Values = z.infer<ReturnType<typeof schemaFor>>;

/**
 * The profile that already owns the number being typed, if any (debounced `GET
 * /customers/phone-check`). The customer being edited does not count as a duplicate of itself.
 * The 409 on submit stays the authority: this only warns earlier.
 */
function usePhoneDuplicate(typed: string, ownId: string | undefined): { id: string; name: string } | null {
  const [digits, setDigits] = useState(() => phoneDigits(typed));
  useEffect(() => {
    const t = setTimeout(() => {
      setDigits(phoneDigits(typed));
    }, PHONE_CHECK_DEBOUNCE_MS);
    return () => {
      clearTimeout(t);
    };
  }, [typed]);
  const check = useQuery(phoneCheckQuery(digits));
  // The answer belongs to the number on screen only once the debounce caught up with it.
  if (digits !== phoneDigits(typed) || !check.data?.taken || !check.data.existing_id) return null;
  if (check.data.existing_id === ownId) return null;
  return { id: check.data.existing_id, name: check.data.full_name ?? "khách khác" };
}

/** Prefill from what the receptionist typed in search: digits → phone, otherwise → name. */
export function prefillFrom(
  typed: string,
): Pick<Values, "full_name" | "phone"> {
  const t = typed.trim();
  return /^[\d\s.+()-]+$/.test(t)
    ? { full_name: "", phone: t }
    : { full_name: t, phone: "" };
}

const opt = (v: string) => (v.trim() === "" ? null : v.trim());

/**
 * Create (no `customer`) or edit a profile. The health note is only part of *creation*; once a
 * customer exists it is edited separately (HealthNoteEditor) because it has its own permission.
 */
export function CustomerForm({
  customer,
  typed = "",
  prefill,
  canWriteHealth,
  onSaved,
  onOpenExisting,
  onCancel,
}: {
  customer?: Customer;
  typed?: string;
  /** Name and phone already known (a newcomer booked by phone): wins over `typed`. */
  prefill?: { full_name: string; phone: string };
  canWriteHealth: boolean;
  onSaved: (c: Customer) => void;
  onOpenExisting?: (id: string) => void;
  onCancel: () => void;
}) {
  const queryClient = useQueryClient();
  const id = useId();
  const [today] = useState(spaToday);
  const bounds = birthDateBounds(today);
  const form = useForm<Values>({
    resolver: zodResolver(schemaFor(today)),
    defaultValues: {
      ...(customer
        ? { full_name: customer.named ? customer.full_name : "", phone: customer.phone ?? "" }
        : (prefill ?? prefillFrom(typed))),
      gender: customer?.gender ?? "",
      birth_date: customer?.birth_date ?? "",
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
        full_name: opt(v.full_name),
        phone: opt(v.phone),
        gender: v.gender === "" ? null : v.gender,
        birth_date: v.birth_date === "" ? null : v.birth_date,
        // With a date of birth the server takes the year from it.
        birth_year: v.birth_date !== "" || v.birth_year === "" ? null : Number(v.birth_year),
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
    onSuccess: (saved) => {
      // The saved profile comes back in the reply: the form closes now, lists refresh behind it.
      void queryClient.invalidateQueries({ queryKey: ["customers"] });
      onSaved(saved);
    },
  });
  const { errors } = form.formState;
  const duplicate = usePhoneDuplicate(useWatch({ control: form.control, name: "phone" }), customer?.id);
  const birthDate = useWatch({ control: form.control, name: "birth_date" });
  const [birthTextOk, setBirthTextOk] = useState(true);
  const existingId =
    mutation.error instanceof ApiError &&
    mutation.error.code === "customer.phone_taken"
      ? mutation.error.existingId
      : undefined;

  const err = {
    full_name: errors.full_name?.message ?? fieldMessage(mutation.error, "full_name"),
    phone: errors.phone?.message ?? fieldMessage(mutation.error, "phone"),
    birth_year: errors.birth_year?.message ?? fieldMessage(mutation.error, "birth_year"),
    birth_date: errors.birth_date?.message ?? fieldMessage(mutation.error, "birth_date"),
  };
  const describe = (field: keyof typeof err, hint?: string) =>
    [err[field] ? `${id}-${field}-error` : undefined, hint].filter(Boolean).join(" ") || undefined;

  return (
    <form
      aria-label={customer ? "Sửa hồ sơ khách" : "Khách mới"}
      noValidate
      onSubmit={(e) =>
        void form.handleSubmit((v) => {
          // The date field shows typed text it could not read: never save the profile without it.
          if (!birthTextOk) {
            form.setError("birth_date", { message: BIRTH_DATE_INVALID });
            return;
          }
          if (!mutation.isPending) mutation.mutate(v);
        })(e)
      }
      className="form-grid"
    >
      <div className="field">
        <label htmlFor={`${id}-name`}>Họ tên</label>
        <input
          id={`${id}-name`}
          className="input"
          autoComplete="off"
          aria-invalid={err.full_name ? true : undefined}
          aria-describedby={describe("full_name", `${id}-name-hint`)}
          {...form.register("full_name")}
        />
        <span id={`${id}-name-hint`} className="field-hint">
          Không bắt buộc. Khách không muốn cho tên thì để trống: hồ sơ hiện là “Khách vãng lai” (bán gói liệu trình cần có tên).
        </span>
        {err.full_name && (
          <span id={`${id}-full_name-error`} className="field-error">
            {err.full_name}
          </span>
        )}
      </div>
      <div className="field">
        <label htmlFor={`${id}-phone`}>Số điện thoại</label>
        <input
          id={`${id}-phone`}
          className="input tabular"
          type="tel"
          inputMode="tel"
          autoComplete="off"
          aria-invalid={err.phone ? true : undefined}
          aria-describedby={`${err.phone ? `${id}-phone-error` : `${id}-phone-hint`} ${id}-phone-duplicate`}
          {...form.register("phone")}
        />
        {err.phone ? (
          <span id={`${id}-phone-error`} className="field-error">
            {err.phone}
          </span>
        ) : (
          <span id={`${id}-phone-hint`} className="field-hint">
            Không bắt buộc. Có số thì kiểm tra trùng với hồ sơ khác.
          </span>
        )}
        {/* Polite live region: announced once the check answers, before the user submits. */}
        <span id={`${id}-phone-duplicate`} aria-live="polite" className="grid">
          {duplicate && !existingId && (
            <span className="callout field-notice">
              <span>
                Số này đã có hồ sơ: <strong>{duplicate.name}</strong>
              </span>
              {onOpenExisting && (
                <button
                  type="button"
                  className="btn btn-ghost"
                  onClick={() => {
                    onOpenExisting(duplicate.id);
                  }}
                >
                  Mở hồ sơ
                </button>
              )}
            </span>
          )}
        </span>
      </div>
      <fieldset>
        <legend>Giới tính (để tính giá)</legend>
        <div className="choice-row">
          {(["female", "male", ""] as const).map((g) => (
            <label key={g} className="choice">
              <input type="radio" value={g} {...form.register("gender")} />
              {g === "female" ? "Nữ" : g === "male" ? "Nam" : "Chưa rõ"}
            </label>
          ))}
        </div>
      </fieldset>
      <div className="field">
        <Controller
          control={form.control}
          name="birth_date"
          render={({ field }) => (
            <DateField
              label="Ngày sinh"
              className="grid gap-1"
              value={field.value}
              onChange={field.onChange}
              onTextValidity={(ok) => {
                setBirthTextOk(ok);
                if (ok) form.clearErrors("birth_date");
              }}
              min={bounds.min}
              max={bounds.max}
              invalid={Boolean(err.birth_date)}
              describedBy={err.birth_date ? `${id}-birth_date-error` : `${id}-birth_date-hint`}
            />
          )}
        />
        {err.birth_date ? (
          <span id={`${id}-birth_date-error`} className="field-error">
            {err.birth_date}
          </span>
        ) : (
          <span id={`${id}-birth_date-hint`} className="field-hint">
            Không bắt buộc. Để sau này tặng ưu đãi sinh nhật.
          </span>
        )}
      </div>
      {birthDate === "" && (
        <div className="field">
          <label htmlFor={`${id}-birth`}>Năm sinh (nếu không biết ngày)</label>
          <input
            id={`${id}-birth`}
            className="input tabular"
            inputMode="numeric"
            autoComplete="off"
            placeholder="Ví dụ: 1985"
            aria-invalid={err.birth_year ? true : undefined}
            aria-describedby={describe("birth_year")}
            {...form.register("birth_year")}
          />
          {err.birth_year && (
            <span id={`${id}-birth_year-error`} className="field-error">
              {err.birth_year}
            </span>
          )}
        </div>
      )}
      <div className="field span-2">
        <label htmlFor={`${id}-address`}>Địa chỉ</label>
        <input id={`${id}-address`} className="input" autoComplete="off" {...form.register("address")} />
      </div>
      <div className="field span-2">
        <label htmlFor={`${id}-note`}>Ghi chú chung</label>
        <input id={`${id}-note`} className="input" autoComplete="off" {...form.register("note")} />
      </div>
      {!customer && canWriteHealth && (
        <div className="field span-2">
          <label htmlFor={`${id}-health`}>Lưu ý sức khỏe (chống chỉ định, tình trạng)</label>
          <textarea id={`${id}-health`} className="input" aria-describedby={`${id}-health-hint`} {...form.register("health_note")} />
          <span id={`${id}-health-hint`} className="field-hint">
            Hiện ở quầy mỗi lần khách đến. Không bắt buộc.
          </span>
        </div>
      )}
      {mutation.isError && (
        <p role="alert" className="alert span-2">
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
      <div className="button-row span-2">
        <button type="submit" aria-busy={mutation.isPending || undefined} className="btn btn-primary">
          {customer ? "Lưu thay đổi" : "Lưu khách"}
        </button>

        <button type="button" onClick={onCancel} className="btn btn-secondary">
          Hủy
        </button>
      </div>
    </form>
  );
}
