import type { Customer } from "@vpyd/api-client";
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { useState } from "react";
import { messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { setHealthNote } from "./api";
import { CustomerPackages } from "../sales/CustomerPackages";
import { CustomerForm } from "./CustomerForm";
import { HealthNote } from "./HealthNote";

function HealthNoteEditor({
  customer,
  onSaved,
}: {
  customer: Customer;
  onSaved: (c: Customer) => void;
}) {
  const queryClient = useQueryClient();
  const [text, setText] = useState(customer.health_note ?? "");
  const mutation = useMutation({
    mutationFn: () =>
      setHealthNote(
        customer.id,
        customer.version,
        text.trim() === "" ? null : text.trim(),
      ),
    onSuccess: async (saved) => {
      await queryClient.invalidateQueries({ queryKey: ["customers"] });
      onSaved(saved);
    },
  });
  return (
    <form
      aria-label="Sửa lưu ý sức khỏe"
      onSubmit={(e) => {
        e.preventDefault();
        mutation.mutate();
      }}
      className="space-y-2"
    >
      <label className="block">
        <span className="text-sm font-semibold">Lưu ý sức khỏe</span>
        <textarea
          value={text}
          onChange={(e) => {
            setText(e.target.value);
          }}
          className="min-h-20 w-full rounded-[6px] border border-border bg-surface px-3 py-2"
        />
      </label>
      {mutation.isError && (
        <p role="alert" className="text-sm text-danger">
          {messageFor(mutation.error)}
        </p>
      )}
      <button
        type="submit"
        disabled={mutation.isPending}
        className="min-h-11 rounded-[10px] border border-border px-4 font-medium hover:bg-surface-2"
      >
        Lưu lưu ý sức khỏe
      </button>
    </form>
  );
}

/** Customer card + edit (profile page) with packages; visit history joins in S7. */
export function CustomerProfile({
  customer,
  onChanged,
}: {
  customer: Customer;
  onChanged: (c: Customer) => void;
}) {
  const { data: me } = useSuspenseQuery(meQuery);
  const [editing, setEditing] = useState(false);
  const canWrite = can(me, "customer.write");

  return (
    <article
      aria-labelledby="customer-name"
      className="rounded-[14px] border border-border bg-surface p-4 shadow-card"
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        {/* Name and gender are the page title (PageHeader); the card starts with the contact line. */}
        <p className="text-text-muted tabular">
          {customer.phone}
          {customer.birth_year ? ` · sinh năm ${String(customer.birth_year)}` : ""}
        </p>
        {canWrite && !editing && (
          <button
            type="button"
            onClick={() => {
              setEditing(true);
            }}
            className="min-h-11 rounded-[10px] border border-border px-4 font-medium hover:bg-surface-2"
          >
            Sửa hồ sơ
          </button>
        )}
      </div>
      <HealthNote customer={customer} />
      {customer.note && <p className="text-sm">Ghi chú: {customer.note}</p>}
      {customer.address && (
        <p className="text-sm text-text-muted">Địa chỉ: {customer.address}</p>
      )}
      <CustomerPackages customerId={customer.id} />
      {editing && (
        <div className="mt-3 grid content-start gap-3">
          <CustomerForm
            customer={customer}
            canWriteHealth={false}
            onSaved={(c) => {
              setEditing(false);
              onChanged(c);
            }}
            onCancel={() => {
              setEditing(false);
            }}
          />
          {can(me, "customer.health_notes.write") && (
            <HealthNoteEditor customer={customer} onSaved={onChanged} />
          )}
        </div>
      )}
    </article>
  );
}
