import type { CustomerPackage } from "@vpyd/api-client";
import { useQuery, useSuspenseQuery } from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import { Suspense, lazy, useState } from "react";
import { Icon } from "../../app/Icon";
import { BottomSheet } from "../../components/BottomSheet";
import { can, meQuery } from "../auth/session";
import { spaToday } from "../corrections/api";
import { useToast } from "../customers/Toast";
import { ErrorState } from "../reports/ui";
import { customerPackagesQuery, formatDate } from "./api";
import { PACKAGE_STATE, daysBetween, isClosedPackage, packageState } from "./packageState";
import { SellPackageDialog } from "./SellPackageDialog";
import { expiryText, serviceLabel } from "../../lib/names";

// Lifecycle forms (extend, cancel + refund, sale void) load only for users who can act.
const PackageActions = lazy(() =>
  import("../corrections/PackageActions").then((m) => ({
    default: m.PackageActions,
  })),
);

function PackageRow({ p, today, canAct, onDone }: { p: CustomerPackage; today: string; canAct: boolean; onDone: (m: string) => void }) {
  const state = packageState(p, today);
  const { label, badge } = PACKAGE_STATE[state];
  const days = p.expires_on ? daysBetween(today, p.expires_on) : null;
  return (
    <li className="pkg-item">
      <div className="row-head">
        <span className="row-title">{p.name}</span>
        <span className={`badge ${badge}`}>{label}</span>
      </div>
      <p className="row-meta">
        <strong>
          Còn {String(p.sessions_remaining)}/{String(p.sessions_total)} buổi
        </strong>
        {" · "}
        {expiryText(p.expires_on, formatDate)}
        {state === "ending" && days != null && days >= 0 && days <= 30 ? ` (còn ${String(days)} ngày)` : ""}
        {p.gift_note ? ` · Quà: ${p.gift_note}` : ""}
      </p>
      {p.items.length > 1 && (
        <p className="row-meta">
          {p.items.map((i) => `${serviceLabel(i.name, i.category_name)} ${String(i.sessions_remaining)}/${String(i.sessions_total)}`).join(" · ")}
        </p>
      )}
      {p.status === "cancelled" && p.cancel_reason && (
        <p className="row-meta">
          Đã hủy: {p.cancel_reason}
          {p.refund ? ` · Hoàn ${formatVnd(p.refund.amount)} ${p.refund.method === "cash" ? "tiền mặt" : "chuyển khoản"}` : ""}
        </p>
      )}
      {canAct && (
        <Suspense fallback={null}>
          <PackageActions pkg={p} onDone={onDone} />
        </Suspense>
      )}
    </li>
  );
}

/**
 * Packages on the customer profile: open packages first (one state chip each: đang dùng / sắp hết
 * / hết hạn), used-up and cancelled ones behind "Gói đã hết / đã hủy (N)". "Bán gói liệu trình"
 * for sellers; lifecycle actions for those allowed. Confirmations are a top toast.
 */
export function CustomerPackages({ customerId }: { customerId: string }) {
  const { data: me } = useSuspenseQuery(meQuery);
  const packages = useQuery(customerPackagesQuery(customerId));
  const [selling, setSelling] = useState(false);
  const [notify, toast] = useToast();
  const today = spaToday();
  const canSell = can(me, "package.sell");
  const canAct = can(me, "package.extend_expiry") || can(me, "package.cancel_refund") || can(me, "record.void_same_day");
  const open = packages.data?.filter((p) => !isClosedPackage(p)) ?? [];
  const closed = packages.data?.filter(isClosedPackage) ?? [];

  return (
    <section aria-labelledby="packages-title" className="panel">
      <div className="panel-head">
        <h2 id="packages-title" className="panel-title">
          Gói liệu trình
        </h2>
        {canSell && (
          <button
            type="button"
            onClick={() => {
              setSelling(true);
            }}
            className="btn btn-primary"
          >
            <Icon name="plus" className="size-4" /> Bán gói liệu trình
          </button>
        )}
      </div>
      {toast}
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
            notify(p ? `Đã bán "${p.name}" — còn ${String(p.sessions_remaining)}/${String(p.sessions_total)} buổi.` : "Đã lưu hóa đơn.");
          }}
        />
      </BottomSheet>
      {packages.isPending && (
        <div role="status" aria-label="Đang tải gói liệu trình" className="grid gap-3">
          <div className="skeleton h-14" />
          <div className="skeleton h-14" />
        </div>
      )}
      {packages.isError && <ErrorState error={packages.error} onRetry={() => void packages.refetch()} />}
      {packages.data?.length === 0 && (
        <p className="empty-note">Chưa có gói liệu trình nào.{canSell ? " Bấm “Bán gói liệu trình” để bán gói đầu tiên cho khách." : ""}</p>
      )}
      {packages.data && packages.data.length > 0 && open.length === 0 && (
        <p className="empty-note">Không còn gói nào đang dùng.{canSell ? " Bấm “Bán gói liệu trình” để bán gói mới." : ""}</p>
      )}
      {open.length > 0 && (
        <ul aria-label="Gói liệu trình của khách" className="row-list">
          {open.map((p) => (
            <PackageRow key={p.id} p={p} today={today} canAct={canAct} onDone={notify} />
          ))}
        </ul>
      )}
      {closed.length > 0 && (
        <details className="disclosure">
          <summary>
            Gói đã hết / đã hủy ({closed.length})
          </summary>
          <ul aria-label="Gói đã hết hoặc đã hủy" className="row-list">
            {closed.map((p) => (
              <PackageRow key={p.id} p={p} today={today} canAct={false} onDone={notify} />
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}
