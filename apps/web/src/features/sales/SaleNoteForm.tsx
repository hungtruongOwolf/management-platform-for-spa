// "Sửa ghi chú gói" (F4): the note of one package sale, edited inline wherever the sale is shown —
// Quầy's "Bán gói không kèm lượt", a visit's packages in the day list, the customer timeline.
// One form, one call (`PATCH /sales/{id}`), the same hint everywhere. Loaded lazily by screens that
// are not already lazy chunks; the server decides who may save (see `mayEditSaleNote`).
import type { Sale } from "@vpyd/api-client";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState } from "react";
import { api, ifMatch } from "../../lib/api";
import { fieldMessage, messageFor } from "../../lib/errors";
import { invalidateManagementViews } from "../../lib/invalidate";

/**
 * `PATCH /sales/{id}`: replaces a package sale's note ("" = none). `version` is the sale's version
 * as the screen read it (`If-Match`): a sale changed since is refused (412), never overwritten.
 */
export async function saveSaleNote(saleId: string, note: string, version: number): Promise<Sale> {
  return (await api(`/sales/${saleId}`, {
    method: "PATCH",
    body: { note: note.trim() === "" ? null : note.trim() },
    headers: ifMatch(version),
  })) as Sale;
}

/**
 * The inline form. On save, every list that shows the note is refreshed (the day lists, the
 * customer's packages and timeline) before `onDone`.
 */
export function SaleNoteForm({
  saleId,
  version,
  note: initial,
  customerId,
  title,
  onDone,
  onCancel,
}: {
  saleId: string;
  /** The sale's `version` as the caller read it (sent as `If-Match`). */
  version: number;
  /** The note as the caller shows it (null / undefined = none yet). */
  note: string | null | undefined;
  /** Whose views to refresh (the payer; the beneficiary's timeline refreshes with them). */
  customerId: string;
  title: string;
  onDone: () => void;
  onCancel: () => void;
}) {
  const queryClient = useQueryClient();
  const titleId = useId();
  const noteId = useId();
  const ref = useRef<HTMLTextAreaElement>(null);
  const [note, setNote] = useState(initial ?? "");
  const save = useMutation({
    mutationFn: () => saveSaleNote(saleId, note, version),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["sales", "day"] });
      void queryClient.invalidateQueries({ queryKey: ["visits"] });
      void queryClient.invalidateQueries({ queryKey: ["customer-packages"] });
      void invalidateManagementViews(queryClient, customerId);
      onDone();
    },
  });
  useEffect(() => {
    ref.current?.focus();
  }, []);
  const problem = fieldMessage(save.error, "note");
  return (
    <section
      role="dialog"
      aria-modal="false"
      aria-labelledby={titleId}
      onKeyDown={(e) => {
        if (e.key === "Escape") onCancel();
      }}
      className="mt-2 grid gap-3 rounded-md border border-border bg-surface p-3"
    >
      <h3 id={titleId} className="font-semibold">
        {title}
      </h3>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          save.mutate();
        }}
        className="grid gap-2"
      >
        <label htmlFor={noteId} className="field">
          Ghi chú gói
        </label>
        <textarea
          id={noteId}
          ref={ref}
          value={note}
          maxLength={2000}
          rows={3}
          aria-invalid={problem ? true : undefined}
          aria-describedby={problem ? `${noteId}-hint ${noteId}-error` : `${noteId}-hint`}
          onChange={(e) => {
            setNote(e.target.value);
          }}
          className="input"
        />
        <span id={`${noteId}-hint`} className="field-hint">
          Bảo mật như ghi chú buổi: chỉ người được xem lưu ý sức khỏe mới đọc
          được. Để trống để xóa ghi chú.
        </span>
        {problem && (
          <p id={`${noteId}-error`} className="field-error">
            {problem}
          </p>
        )}
        {save.isError && !problem && (
          <p role="alert" className="alert">
            {messageFor(save.error)}
          </p>
        )}
        <div className="button-row">
          <button
            type="submit"
            disabled={save.isPending}
            className="btn btn-primary"
          >
            {save.isPending ? "Đang lưu…" : "Lưu ghi chú"}
          </button>
          <button
            type="button"
            onClick={onCancel}
            className="btn btn-secondary"
          >
            Thôi
          </button>
        </div>
      </form>
    </section>
  );
}
