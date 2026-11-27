import { useId, useState, type ReactNode } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { fieldMessage, messageFor } from "../../lib/errors";
import { DRAW_FIELDS } from "./messages";
import { chars } from "./roster";

export const DRAW_NAME_MAX = 80;

/** What a sheet reads of the mutation it runs. */
export interface Pending {
  isPending: boolean;
  error: Error | null;
}

/** Refusals of a whole list rather than of one field (`errors[].field` of the API). */
const LIMITS: Record<string, string> = {
  prizes: "Một chương trình có tối đa 50 giải thưởng.",
  participants: "Một chương trình có tối đa 2.000 người tham gia.",
};

/** The server's refusal of a form, under it, unless a field already says it. */
export function FormError({ error, fields = [] }: { error: unknown; fields?: readonly string[] }) {
  if (!error || fields.some((f) => fieldMessage(error, f, DRAW_FIELDS))) return null;
  const limit = Object.keys(LIMITS).find((f) => fieldMessage(error, f) !== undefined);
  return (
    <p role="alert" className="alert">
      {limit ? LIMITS[limit] : messageFor(error)}
    </p>
  );
}

/** "Tạo chương trình" and "Đổi tên" (AC1.2, AC1.4): one field, the program's name. */
export function NameSheet({
  title,
  submit,
  initial = "",
  mutation,
  onClose,
}: {
  title: string;
  submit: string;
  initial?: string;
  mutation: Pending & { mutate: (name: string) => void };
  onClose: () => void;
}) {
  const id = useId();
  const [name, setName] = useState(initial);
  const [touched, setTouched] = useState(false);
  const trimmed = name.trim();
  const problem =
    (touched && trimmed === "" ? "Nhập tên chương trình." : undefined) ??
    (chars(trimmed) > DRAW_NAME_MAX ? `Tên dài quá ${String(DRAW_NAME_MAX)} ký tự.` : undefined) ??
    fieldMessage(mutation.error, "name", DRAW_FIELDS);
  return (
    <BottomSheet
      open
      onClose={onClose}
      title={title}
      footer={
        <>
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            Hủy
          </button>
          <button type="submit" form={id} className="btn btn-primary" aria-busy={mutation.isPending || undefined}>
            {submit}
          </button>
        </>
      }
    >
      <form
        id={id}
        noValidate
        className="grid gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          setTouched(true);
          if (trimmed === "" || chars(trimmed) > DRAW_NAME_MAX || mutation.isPending) return;
          if (trimmed === initial) onClose();
          else mutation.mutate(trimmed);
        }}
      >
        <div className="field">
          <label htmlFor={`${id}-name`}>Tên chương trình</label>
          <input
            id={`${id}-name`}
            className="input"
            value={name}
            autoComplete="off"
            // The sheet holds one field: typing is the only thing to do.
            autoFocus
            aria-invalid={problem ? true : undefined}
            aria-describedby={`${id}-help`}
            onChange={(e) => {
              setName(e.target.value);
            }}
          />
          <span id={`${id}-help`} className={problem ? "field-error" : "field-hint"}>
            {problem ?? "Ví dụ: Tri ân 20/11."}
          </span>
        </div>
        <FormError error={mutation.error} fields={["name"]} />
      </form>
    </BottomSheet>
  );
}

/** "Xóa …?" / "Trả giải …?": what happens, then the button that does it. */
export function ConfirmSheet({
  title,
  children,
  confirm,
  danger = false,
  mutation,
  onConfirm,
  onClose,
}: {
  title: string;
  children: ReactNode;
  confirm: string;
  danger?: boolean;
  mutation: Pending;
  onConfirm: () => void;
  onClose: () => void;
}) {
  return (
    <BottomSheet
      open
      onClose={onClose}
      title={title}
      footer={
        <>
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            Thôi
          </button>
          <button
            type="button"
            className={`btn ${danger ? "btn-danger" : "btn-primary"}`}
            aria-busy={mutation.isPending || undefined}
            onClick={() => {
              if (!mutation.isPending) onConfirm();
            }}
          >
            {confirm}
          </button>
        </>
      }
    >
      <div className="grid gap-3">
        <p>{children}</p>
        <FormError error={mutation.error} />
      </div>
    </BottomSheet>
  );
}
