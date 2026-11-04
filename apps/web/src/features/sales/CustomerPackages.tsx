import { useQuery, useSuspenseQuery } from "@tanstack/react-query";
import { useState } from "react";
import { messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { STATUS_LABEL, customerPackagesQuery, formatDate } from "./api";
import { SellPackageDialog } from "./SellPackageDialog";

/** Packages on the customer card: remaining / total sessions, expiry, status; "Bán liệu trình mới" for sellers. */
export function CustomerPackages({ customerId }: { customerId: string }) {
  const { data: me } = useSuspenseQuery(meQuery);
  const packages = useQuery(customerPackagesQuery(customerId));
  const [selling, setSelling] = useState(false);
  const [sold, setSold] = useState<string | null>(null);

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
                {p.origin === "opening_balance" && (
                  <span className="ml-2 rounded-full bg-surface-2 px-2 text-xs">
                    Nhập từ sổ cũ
                  </span>
                )}
                <span className="block text-sm text-text-muted">
                  {p.items.length > 1 &&
                    `${p.items.map((i) => `${i.name} ${String(i.sessions_remaining)}/${String(i.sessions_total)}`).join(" · ")} · `}
                  {p.expires_on
                    ? `HSD ${formatDate(p.expires_on)}`
                    : "Không thời hạn"}
                  {p.gift_note ? ` · Quà: ${p.gift_note}` : ""}
                </span>
              </div>
              <span className="text-right">
                <strong>
                  còn {String(p.sessions_remaining)}/{String(p.sessions_total)}{" "}
                  buổi
                </strong>
                <span className="block text-xs text-text-muted">
                  {STATUS_LABEL[p.status]}
                </span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
