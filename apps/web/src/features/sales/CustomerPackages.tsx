import { useQuery, useSuspenseQuery } from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import { Suspense, lazy, useState } from "react";
import { Icon } from "../../app/Icon";
import { BottomSheet } from "../../components/BottomSheet";
import { messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import {
  STATUS_BADGE,
  STATUS_LABEL,
  customerPackagesQuery,
  formatDate,
} from "./api";
import { SellPackageDialog } from "./SellPackageDialog";
import { expiryText, serviceLabel } from "../../lib/names";

// Lifecycle forms (extend, cancel + refund, sale void) load only for users who can act.
const PackageActions = lazy(() =>
  import("../corrections/PackageActions").then((m) => ({
    default: m.PackageActions,
  })),
);

/**
 * Packages on the customer card: remaining / total sessions, expiry, status; "Bán gói liệu trình"
 * for sellers; lifecycle actions (extend, cancel + refund, void the sale) for those allowed.
 */
export function CustomerPackages({ customerId }: { customerId: string }) {
  const { data: me } = useSuspenseQuery(meQuery);
  const packages = useQuery(customerPackagesQuery(customerId));
  const [selling, setSelling] = useState(false);
  const [sold, setSold] = useState<string | null>(null);
  const canAct =
    can(me, "package.extend_expiry") ||
    can(me, "package.cancel_refund") ||
    can(me, "record.void_same_day");

  return (
    <section aria-labelledby="packages-title" className="card grid content-start gap-3 p-4">
      <div className="card-head">
        <h2 id="packages-title" className="card-title">
          <Icon name="package" className="size-5 text-accent" /> Gói liệu trình
        </h2>
        {can(me, "package.sell") && !selling && (
          <button
            type="button"
            onClick={() => {
              setSold(null);
              setSelling(true);
            }}
            className="btn btn-primary"
          >
            <Icon name="plus" className="size-4" /> Bán gói liệu trình
          </button>
        )}
      </div>
      {sold && (
        <p role="status" className="rounded-[10px] bg-primary-soft p-2 text-sm">
          {sold}
        </p>
      )}
      <BottomSheet
        open={selling}
        onClose={() => {
          setSelling(false);
        }}
        title="Bán gói liệu trình"
        wide
      >
        <SellPackageDialog
          customerId={customerId}
          onCancel={() => {
            setSelling(false);
          }}
          onDone={(sale) => {
            setSelling(false);
            const p = sale.customer_packages[0];
            setSold(
              p
                ? `Đã bán "${p.name}" — còn ${String(p.sessions_remaining)}/${String(p.sessions_total)} buổi.`
                : "Đã lưu hóa đơn.",
            );
          }}
        />
      </BottomSheet>
      {packages.isPending && (
        <p role="status" className="text-text-muted">
          Đang tải gói liệu trình…
        </p>
      )}
      {packages.isError && (
        <p role="alert" className="text-danger">
          {messageFor(packages.error)}
        </p>
      )}
      {packages.data?.length === 0 && (
        <p className="text-sm text-text-muted">Chưa có gói liệu trình nào.</p>
      )}
      {packages.data && packages.data.length > 0 && (
        <ul
          aria-label="Gói liệu trình của khách"
          className="divide-y divide-border rounded-[10px] border border-border"
        >
          {packages.data.map((p) => (
            <li key={p.id} className="package-row">
              <div className="package-row-head">
                <span className="font-semibold">{p.name}</span>
                <span className={`badge ${STATUS_BADGE[p.status]}`}>{STATUS_LABEL[p.status]}</span>
              </div>
              <p className="package-row-meta">
                <strong>
                  Còn {String(p.sessions_remaining)}/{String(p.sessions_total)} buổi
                </strong>
                {" · "}
                {expiryText(p.expires_on, formatDate)}
                {p.gift_note ? ` · Quà: ${p.gift_note}` : ""}
              </p>
              {p.items.length > 1 && (
                <p className="package-row-meta">
                  {p.items.map((i) => `${serviceLabel(i.name, i.category_name)} ${String(i.sessions_remaining)}/${String(i.sessions_total)}`).join(" · ")}
                </p>
              )}
              {p.status === "cancelled" && p.cancel_reason && (
                <p className="text-sm text-danger">
                  Đã hủy: {p.cancel_reason}
                  {p.refund
                    ? ` · Hoàn ${formatVnd(p.refund.amount)} ${p.refund.method === "cash" ? "tiền mặt" : "chuyển khoản"}`
                    : ""}
                </p>
              )}
              {canAct && (
                <Suspense fallback={null}>
                  <PackageActions
                    pkg={p}
                    onDone={(message) => {
                      setSold(message);
                    }}
                  />
                </Suspense>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
