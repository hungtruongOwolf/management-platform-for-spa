import type { PackagePlan } from "@vpyd/api-client";
import { formatVnd } from "@vpyd/ui";
import { useQueries, useSuspenseQuery } from "@tanstack/react-query";
import { useState } from "react";
import { messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { plansQuery, servicesQuery } from "./api";
import { PlanForm } from "./PlanForm";

type Editing = { plan?: PackagePlan } | null;

/** /bang-gia/goi — package plans (liệu trình). Owners create and edit; others read. */
export function PlansPage() {
  const { data: me } = useSuspenseQuery(meQuery);
  const canManage = can(me, "catalog.manage");
  const [plans, services] = useQueries({
    queries: [plansQuery(canManage), servicesQuery(canManage)],
  });
  const [editing, setEditing] = useState<Editing>(null);

  if (plans.isPending || services.isPending)
    return (
      <p role="status" className="text-text-muted">
        Đang tải gói liệu trình…
      </p>
    );
  if (plans.isError || services.isError) {
    return (
      <p role="alert" className="rounded-[10px] bg-danger-soft p-3 text-danger">
        {messageFor(plans.error ?? services.error)}
      </p>
    );
  }
  const serviceName = new Map(services.data.map((s) => [s.id, s.name]));

  return (
    <section aria-labelledby="plans-title" className="grid content-start gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h1 id="plans-title" className="font-brand text-2xl text-primary">
          Gói liệu trình
        </h1>
        {canManage && editing === null && (
          <button
            type="button"
            onClick={() => {
              setEditing({});
            }}
            className="min-h-11 rounded-[10px] bg-primary px-4 font-semibold text-on-primary"
          >
            + Tạo gói
          </button>
        )}
      </div>
      {editing && editing.plan === undefined && (
        <PlanForm
          services={services.data}
          onDone={() => {
            setEditing(null);
          }}
        />
      )}
      {plans.data.length === 0 && (
        <p className="text-text-muted">Chưa có gói liệu trình nào.</p>
      )}
      <ul className="grid content-start gap-2">
        {plans.data.map((plan) => (
          <li
            key={plan.id}
            className={`rounded-[14px] border border-border bg-surface p-4 shadow-card ${plan.active ? "" : "border-dashed text-text-muted"}`}
          >
            <div className="flex flex-wrap items-baseline justify-between gap-2">
              <div>
                <p className="font-semibold">
                  {plan.name}
                  {!plan.active && (
                    <span className="ml-2 rounded-full bg-surface-2 px-2 text-xs text-text-muted">
                      Ngừng bán
                    </span>
                  )}
                </p>
                <p className="text-sm text-text-muted">
                  {plan.items
                    .map(
                      (i) =>
                        `${serviceName.get(i.service_id) ?? "?"} × ${String(i.sessions)}${i.bonus_sessions ? ` + tặng ${String(i.bonus_sessions)}` : ""}`,
                    )
                    .join(" · ")}
                  {" · "}
                  {plan.validity_months
                    ? `Hạn ${String(plan.validity_months)} tháng`
                    : "Không thời hạn"}
                  {plan.gift_note ? ` · Quà: ${plan.gift_note}` : ""}
                </p>
              </div>
              <p className="flex items-center gap-3">
                <span className="font-semibold tabular-nums">
                  {formatVnd(plan.price)}
                </span>
                {canManage && (
                  <button
                    type="button"
                    onClick={() => {
                      setEditing({ plan });
                    }}
                    aria-label={`Sửa gói ${plan.name}`}
                    className="min-h-11 rounded-[10px] border border-border px-3 text-sm hover:bg-surface-2"
                  >
                    Sửa
                  </button>
                )}
              </p>
            </div>
            {editing?.plan?.id === plan.id && (
              <PlanForm
                services={services.data}
                plan={plan}
                onDone={() => {
                  setEditing(null);
                }}
              />
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
