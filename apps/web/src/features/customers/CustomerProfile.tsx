import type { Customer } from "@vpyd/api-client";
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { useState } from "react";
import { Icon } from "../../app/Icon";
import { BottomSheet } from "../../components/BottomSheet";
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
      className="grid content-start gap-2 border-t border-border pt-4"
    >
      <label className="field">
        Lưu ý sức khỏe
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
      <button type="submit" disabled={mutation.isPending} className="btn btn-secondary">
        Lưu lưu ý sức khỏe
      </button>
    </form>
  );
}

/** One "label: value" line of the contact card; nothing when the value is empty. */
function Detail({ label, children }: { label: string; children: string | null | undefined }) {
  if (!children) return null;
  return (
    <div className="detail-row">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

/**
 * Left column of the profile: the contact card (details, health note, "Sửa hồ sơ" in a sheet)
 * and the customer's packages. Both cards share the heading style of the history column.
 */
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
    <div className="grid content-start gap-4">
      <article aria-labelledby="customer-info-title" className="card grid content-start gap-3 p-4">
        <div className="card-head">
          <h2 id="customer-info-title" className="card-title">
            <Icon name="users" className="size-5 text-accent" /> Thông tin khách
          </h2>
          {canWrite && (
            <button
              type="button"
              onClick={() => {
                setEditing(true);
              }}
              className="btn btn-secondary"
            >
              Sửa hồ sơ
            </button>
          )}
        </div>
        <HealthNote customer={customer} />
        <dl className="detail-list">
          <Detail label="Số điện thoại">{customer.phone}</Detail>
          <Detail label="Năm sinh">{customer.birth_year ? String(customer.birth_year) : null}</Detail>
          <Detail label="Địa chỉ">{customer.address}</Detail>
          <Detail label="Ghi chú">{customer.note}</Detail>
        </dl>
      </article>
      <CustomerPackages customerId={customer.id} />
      <BottomSheet
        open={editing}
        onClose={() => {
          setEditing(false);
        }}
        title="Sửa hồ sơ khách"
        description={customer.full_name}
        wide
      >
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
      </BottomSheet>
    </div>
  );
}
