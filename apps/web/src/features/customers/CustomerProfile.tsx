import type { Customer } from "@vpyd/api-client";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { useId, useState } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { useAfterFirstPaint } from "../../lib/afterFirstPaint";
import { messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { LazyRecordTrace, TracePlaceholder } from "../record-history/lazy";
import { canTrace } from "../record-history/query";
import { formatDate } from "../sales/api";
import { phoneText, setHealthNote } from "./api";
import { CustomerPackages } from "../sales/CustomerPackages";
import { CustomerForm } from "./CustomerForm";

function HealthNoteEditor({ customer, onSaved, onCancel }: { customer: Customer; onSaved: (c: Customer) => void; onCancel: () => void }) {
  const queryClient = useQueryClient();
  const id = useId();
  const [text, setText] = useState(customer.health_note ?? "");
  const mutation = useMutation({
    mutationFn: () => setHealthNote(customer.id, customer.version, text.trim() === "" ? null : text.trim()),
    onSuccess: (saved) => {
      void queryClient.invalidateQueries({ queryKey: ["customers"] });
      void queryClient.invalidateQueries({ queryKey: ["notes"] });
      onSaved(saved);
    },
  });
  return (
    <form
      aria-label="Sửa lưu ý sức khỏe"
      onSubmit={(e) => {
        e.preventDefault();
        if (!mutation.isPending) mutation.mutate();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") onCancel();
      }}
      className="grid gap-3"
    >
      <div className="field">
        <label htmlFor={`${id}-note`}>Lưu ý sức khỏe (chống chỉ định, tình trạng)</label>
        <textarea
          id={`${id}-note`}
          autoFocus
          value={text}
          // crm MAX_HEALTH_NOTE.
          maxLength={2000}
          aria-describedby={`${id}-hint`}
          onChange={(e) => {
            setText(e.target.value);
          }}
          className="input"
        />
        <span id={`${id}-hint`} className="field-hint">
          Để trống rồi lưu để xóa lưu ý.
        </span>
      </div>
      {mutation.isError && (
        <p role="alert" className="alert">
          {messageFor(mutation.error)}
        </p>
      )}
      <div className="button-row">
        <button type="submit" aria-busy={mutation.isPending || undefined} className="btn btn-primary">
          Lưu lưu ý sức khỏe
        </button>
        <button type="button" onClick={onCancel} className="btn btn-secondary">
          Bỏ qua
        </button>
      </div>
    </form>
  );
}

/** One "label: value" line of the contact details; nothing when the value is empty. */
function Detail({ label, children }: { label: string; children: string | null | undefined }) {
  if (!children) return null;
  return (
    <div>
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

/**
 * The health note section: shown only when the API sent the field (the caller may read it). The
 * warning is the amber callout (same as the front desk); editing opens inline for those allowed.
 */
function HealthSection({ customer, canEdit, onChanged }: { customer: Customer; canEdit: boolean; onChanged: (c: Customer) => void }) {
  const [editing, setEditing] = useState(false);
  if (!("health_note" in customer)) return null;
  const note = customer.health_note;
  return (
    <section aria-labelledby="health-title" className="panel-section">
      <div className="row-head">
        <h3 id="health-title" className="section-label">
          Lưu ý sức khỏe
        </h3>
        {canEdit && !editing && (
          <button
            type="button"
            className="btn btn-ghost"
            onClick={() => {
              setEditing(true);
            }}
          >
            {note ? "Sửa lưu ý sức khỏe" : "Thêm lưu ý sức khỏe"}
          </button>
        )}
      </div>
      {editing ? (
        <HealthNoteEditor
          customer={customer}
          onCancel={() => {
            setEditing(false);
          }}
          onSaved={(c) => {
            setEditing(false);
            onChanged(c);
          }}
        />
      ) : note ? (
        <div role="note" aria-label="Lưu ý sức khỏe" className="health-note">
          <span className="health-note-text">
            <span aria-hidden="true">⚠ </span>
            {note}
          </span>
        </div>
      ) : (
        <p className="empty-note">Chưa ghi lưu ý sức khỏe nào{canEdit ? ". Bấm “Thêm lưu ý sức khỏe” nếu khách có chống chỉ định." : "."}</p>
      )}
    </section>
  );
}

/**
 * Left column of the profile: the contact details (+ "Sửa hồ sơ" in a sheet), the health note
 * section, then the customer's packages.
 */
export function CustomerProfile({ customer, onChanged }: { customer: Customer; onChanged: (c: Customer) => void }) {
  const { data: me } = useSuspenseQuery(meQuery);
  const [editing, setEditing] = useState(false);
  const canWrite = can(me, "customer.write");
  const traced = canTrace(me, "customer");
  // The profile's own requests go first; "ai ghi, ai sửa" is read once it has painted.
  const painted = useAfterFirstPaint();

  return (
    <div className="khach-stack">
      <article aria-labelledby="customer-info-title" className="panel">
        <div className="panel-head">
          {/* Focus lands here when the profile's history is taken away (403). */}
          <h2 id="customer-info-title" tabIndex={traced ? -1 : undefined} className="panel-title">
            Thông tin khách
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
        <dl className="detail-grid">
          <Detail label="Số điện thoại">{phoneText(customer.phone)}</Detail>
          {customer.birth_date ? (
            <Detail label="Ngày sinh">{formatDate(customer.birth_date)}</Detail>
          ) : (
            <Detail label="Năm sinh">{customer.birth_year ? String(customer.birth_year) : null}</Detail>
          )}
          <Detail label="Địa chỉ">{customer.address}</Detail>
          <Detail label="Ghi chú">{customer.note}</Detail>
        </dl>
        {/* "Tạo bởi … · đã sửa … lần" (ADR-0022): the last element of the contact details. */}
        {traced &&
          (painted ? (
            <LazyRecordTrace
              record={{ type: "customer", id: customer.id }}
              noun={`hồ sơ ${customer.full_name}`}
              focusOnHide={() => document.getElementById("customer-info-title")}
            />
          ) : (
            <TracePlaceholder />
          ))}
        <HealthSection customer={customer} canEdit={can(me, "customer.health_notes.write")} onChanged={onChanged} />
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
      </BottomSheet>
    </div>
  );
}
