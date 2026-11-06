import type { Customer } from "@vpyd/api-client";

// Own module so the front desk (main bundle) can show it without pulling in the customer profile,
// its forms and the sell-package dialog (frontend.md, perf budget).

/** Highlighted health warning. Rendered only when the API sent the field (caller has permission). */
export function HealthNote({ customer }: { customer: Customer }) {
  if (!("health_note" in customer) || !customer.health_note) return null;
  return (
    <div
      role="note"
      className="health-note"
    >
      <strong className="health-note-title">
        <span aria-hidden="true">⚠</span> Lưu ý sức khỏe
      </strong>
      <span className="health-note-text">{customer.health_note}</span>
    </div>
  );
}
