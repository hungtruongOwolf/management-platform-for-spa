import type { PackagePlan } from "@vpyd/api-client";
import { formatVnd } from "@vpyd/ui";
import { useQueries, useSuspenseQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { Icon } from "../../app/Icon";
import { PageHeader } from "../../app/PageHeader";
import { can, meQuery } from "../auth/session";
import { usePreloadRecordHistory } from "../record-history/lazy";
import { canTrace } from "../record-history/query";
import { ErrorState } from "../reports/ui";
import { plansQuery, servicesQuery } from "./api";
import { PlanForm } from "./PlanForm";
import "../customers/khach.css";

type Editing = { plan?: PackagePlan } | null;

/** "Gội đầu × 10 + tặng 2 · Hạn 6 tháng · Quà: …" */
function describePlan(plan: PackagePlan, serviceName: Map<string, string>): string {
  const items = plan.items
    .map((i) => `${serviceName.get(i.service_id) ?? "?"} × ${String(i.sessions)}${i.bonus_sessions ? ` + tặng ${String(i.bonus_sessions)}` : ""}`)
    .join(" · ");
  const validity = plan.validity_months ? `Hạn ${String(plan.validity_months)} tháng` : "Không hạn";
  return `${items} · ${validity}${plan.gift_note ? ` · Quà: ${plan.gift_note}` : ""}`;
}

/** /bang-gia/goi — package plans (liệu trình). Owners create and edit; others read. */
export function PlansPage() {
  const { data: me } = useSuspenseQuery(meQuery);
  const canManage = can(me, "catalog.manage");
  // "Ai ghi, ai sửa" at the end of a plan's edit form (ADR-0022 wave 3), never on the list.
  const traced = canTrace(me, "catalog");
  usePreloadRecordHistory(traced);
  const [plans, services] = useQueries({
    queries: [plansQuery(canManage), servicesQuery(canManage)],
  });
  const [editing, setEditing] = useState<Editing>(null);
  const done = () => {
    setEditing(null);
  };

  const header = (
    <PageHeader title="Mẫu gói liệu trình" titleId="plans-title">
      <Link to="/bang-gia" className="btn btn-secondary">
        Bảng giá dịch vụ
      </Link>
      {canManage && editing === null && (
        <button
          type="button"
          onClick={() => {
            setEditing({});
          }}
          className="btn btn-primary"
        >
          <Icon name="plus" className="size-4" /> Tạo mẫu gói
        </button>
      )}
    </PageHeader>
  );

  if (plans.isPending || services.isPending)
    return (
      <section aria-labelledby="plans-title" className="grid content-start gap-6">
        {header}
        <div role="status" aria-label="Đang tải mẫu gói liệu trình" className="card grid content-start gap-3 p-4">
          {[0, 1, 2].map((i) => (
            <div key={i} className="skeleton h-12" />
          ))}
        </div>
      </section>
    );
  if (plans.isError || services.isError) {
    return (
      <section aria-labelledby="plans-title" className="grid content-start gap-6">
        {header}
        <ErrorState
          error={plans.error ?? services.error}
          onRetry={() => {
            void plans.refetch();
            void services.refetch();
          }}
        />
      </section>
    );
  }
  const serviceName = new Map(services.data.map((s) => [s.id, s.name]));
  const selling = plans.data.filter((p) => p.active);
  const stopped = plans.data.filter((p) => !p.active);
  // Same markup as /bang-gia (EM 2026-11-06: "làm UI giống cái bảng giá dịch vụ"): one card per
  // list, rows divided by hairlines, name + details left, price and "Sửa" on the right.
  const row = (plan: PackagePlan) => {
    const open = editing?.plan?.id === plan.id;
    return (
      <li key={plan.id} className="py-2">
        <div className={`flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 ${plan.active ? "" : "text-text-muted"}`}>
          <div className="min-w-0">
            <p className="flex flex-wrap items-center gap-x-2 gap-y-1 font-semibold">
              {plan.name}
              {!plan.active && <span className="badge badge-muted">Ngừng bán</span>}
            </p>
            <p className="text-sm text-text-muted">{describePlan(plan, serviceName)}</p>
          </div>
          <p className="catalog-price flex flex-wrap items-center gap-x-4 gap-y-1 tabular-nums">
            <span className="font-semibold">{formatVnd(plan.price)}</span>
            {canManage && (
              <button
                type="button"
                onClick={() => {
                  setEditing(open ? null : { plan });
                }}
                aria-label={`Sửa mẫu gói ${plan.name}`}
                aria-expanded={open}
                className="btn btn-secondary"
              >
                Sửa
              </button>
            )}
          </p>
        </div>
        {open && <PlanForm services={services.data} plan={plan} onDone={done} traced={traced} />}
      </li>
    );
  };

  return (
    <section aria-labelledby="plans-title" className="grid content-start gap-6">
      {header}
      {editing && editing.plan === undefined && (
        <section aria-label="Tạo mẫu gói" className="card p-4">
          <PlanForm services={services.data} onDone={done} />
        </section>
      )}
      <section aria-labelledby="plans-list-title" className="card p-4">
        <h2 id="plans-list-title" className="mb-3 text-lg font-semibold">
          Đang bán
        </h2>
        {selling.length === 0 && (
          <p className="empty-block">
            Chưa có mẫu gói liệu trình nào đang bán.{canManage ? " Bấm “Tạo mẫu gói” để tạo gói đầu tiên (ví dụ: Gội đầu 10 buổi tặng 2)." : ""}
          </p>
        )}
        {selling.length > 0 && (
          <ul className="divide-y divide-border" aria-label="Mẫu gói đang bán">
            {selling.map(row)}
          </ul>
        )}
      </section>
      {stopped.length > 0 && (
        <section aria-labelledby="plans-stopped-title" className="card p-4">
          <h2 id="plans-stopped-title" className="mb-3 text-lg font-semibold">
            Ngừng bán ({stopped.length})
          </h2>
          <ul className="divide-y divide-border" aria-label="Mẫu gói ngừng bán">
            {stopped.map(row)}
          </ul>
        </section>
      )}
    </section>
  );
}
