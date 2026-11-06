import type { PackagePlan } from "@vpyd/api-client";
import { formatVnd } from "@vpyd/ui";
import { useQueries, useSuspenseQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Icon } from "../../app/Icon";
import { PageHeader } from "../../app/PageHeader";
import { can, meQuery } from "../auth/session";
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
  const [plans, services] = useQueries({
    queries: [plansQuery(canManage), servicesQuery(canManage)],
  });
  const [editing, setEditing] = useState<Editing>(null);
  const done = () => {
    setEditing(null);
  };

  const header = (
    <PageHeader title="Mẫu gói liệu trình" titleId="plans-title">
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
      <section aria-labelledby="plans-title" className="khach-page">
        {header}
        <div role="status" aria-label="Đang tải mẫu gói liệu trình" className="panel">
          {[0, 1, 2].map((i) => (
            <div key={i} className="skeleton h-12" />
          ))}
        </div>
      </section>
    );
  if (plans.isError || services.isError) {
    return (
      <section aria-labelledby="plans-title" className="khach-page">
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
  const row = (plan: PackagePlan) => {
    const open = editing?.plan?.id === plan.id;
    return (
      <li key={plan.id} className={plan.active ? undefined : "is-muted"}>
        <div className="price-row">
          <div className="min-w-0">
            <p className="flex flex-wrap items-center gap-x-2 gap-y-1">
              <span className="row-title">{plan.name}</span>
              {!plan.active && <span className="badge badge-muted">Ngừng bán</span>}
            </p>
            <p className="row-meta">{describePlan(plan, serviceName)}</p>
          </div>
          <p className="price-amounts">
            <span className="price-value">{formatVnd(plan.price)}</span>
          </p>
          {canManage && (
            <span className="row-side">
              <button
                type="button"
                onClick={() => {
                  setEditing(open ? null : { plan });
                }}
                aria-label={`Sửa mẫu gói ${plan.name}`}
                aria-expanded={open}
                className="btn btn-ghost"
              >
                Sửa
              </button>
            </span>
          )}
        </div>
        {open && <PlanForm services={services.data} plan={plan} onDone={done} />}
      </li>
    );
  };

  return (
    <section aria-labelledby="plans-title" className="khach-page">
      {header}
      {editing && editing.plan === undefined && (
        <section aria-label="Tạo mẫu gói" className="panel">
          <PlanForm services={services.data} onDone={done} />
        </section>
      )}
      <section aria-labelledby="plans-list-title" className="panel">
        <h2 id="plans-list-title" className="panel-title">
          Đang bán
        </h2>
        {selling.length === 0 && (
          <p className="khach-empty">
            Chưa có mẫu gói liệu trình nào đang bán.{canManage ? " Bấm “Tạo mẫu gói” để tạo gói đầu tiên (ví dụ: Gội đầu 10 buổi tặng 2)." : ""}
          </p>
        )}
        {selling.length > 0 && (
          <ul className="row-list" aria-label="Mẫu gói đang bán">
            {selling.map(row)}
          </ul>
        )}
        {stopped.length > 0 && (
          <details className="closed-group">
            <summary>
              <Icon name="chevron-right" className="size-4" /> Ngừng bán ({stopped.length})
            </summary>
            <ul className="row-list" aria-label="Mẫu gói ngừng bán">
              {stopped.map(row)}
            </ul>
          </details>
        )}
      </section>
    </section>
  );
}
