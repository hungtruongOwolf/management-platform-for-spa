import { useQuery, useSuspenseQuery } from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import { Suspense, lazy, useState } from "react";
import { messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import {
  STATUS_BADGE,
  STATUS_LABEL,
  customerPackagesQuery,
  formatDate,
} from "./api";
import { SellPackageDialog } from "./SellPackageDialog";

// Lifecycle forms (extend, cancel + refund, sale void) load only for users who can act.
const PackageActions = lazy(() =>
  import("../corrections/PackageActions").then((m) => ({
    default: m.PackageActions,
  })),
);

/**
 * Packages on the customer card: remaining / total sessions, expiry, status; "Bán liệu trình mới"
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
    <section aria-labelledby="packages-title" className="mt-4 space-y-2">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 id="packages-title" className="font-semibold">
          Liệu trình
        </h2>
        {can(me, "package.sell") && !selling && (
          <button
            type="button"
            onClick={() => {
              setSold(null);
              setSelling(true);
            }}
            className="min-h-11 rounded-[10px] bg-primary px-4 font-semibold text-on-primary"
          >
            + Bán liệu trình mới
          </button>
        )}
      </div>
      {sold && (
        <p role="status" className="rounded-[10px] bg-primary-soft p-2 text-sm">
          {sold}
        </p>
      )}
      {selling && (
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
      )}
      {packages.isPending && (
        <p role="status" className="text-text-muted">
          Đang tải liệu trình…
        </p>
      )}
      {packages.isError && (
        <p role="alert" className="text-danger">
          {messageFor(packages.error)}
        </p>
      )}
      {packages.data?.length === 0 && (
        <p className="text-text-muted">Chưa có liệu trình nào.</p>
      )}
      {packages.data && packages.data.length > 0 && (
        <ul
          aria-label="Liệu trình của khách"
          className="divide-y divide-border rounded-[10px] border border-border"
        >
          {packages.data.map((p) => (
            <li
              key={p.id}
              className="flex flex-wrap items-baseline justify-between gap-2 px-3 py-2"
            >
              <div>
                <span className="font-semibold">{p.name}</span>
                <span className="block text-sm text-text-muted">
                  {p.items.length > 1 &&
                    `${p.items.map((i) => `${i.name} ${String(i.sessions_remaining)}/${String(i.sessions_total)}`).join(" · ")} · `}
                  {p.expires_on
                    ? `HSD ${formatDate(p.expires_on)}`
                    : "Không thời hạn"}
                  {p.gift_note ? ` · Quà: ${p.gift_note}` : ""}
                </span>
                {p.status === "cancelled" && p.cancel_reason && (
                  <span className="block text-sm text-danger">
                    Đã hủy: {p.cancel_reason}
                    {p.refund
                      ? ` · Hoàn ${formatVnd(p.refund.amount)} ${p.refund.method === "cash" ? "tiền mặt" : "chuyển khoản"}`
                      : ""}
                  </span>
                )}
              </div>
              <span className="text-right">
                <strong>
                  còn {String(p.sessions_remaining)}/{String(p.sessions_total)}{" "}
                  buổi
                </strong>
                <span
                  className={`mt-0.5 inline-block rounded-full px-2 text-xs font-semibold ${STATUS_BADGE[p.status]}`}
                >
                  {STATUS_LABEL[p.status]}
                </span>
              </span>
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
