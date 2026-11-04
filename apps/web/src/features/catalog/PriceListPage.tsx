import { useQueries } from "@tanstack/react-query";
import { messageFor } from "../../lib/errors";
import { categoriesQuery, servicesQuery } from "./api";
import { describeDuration, describePrices } from "./prices";

/** /bang-gia — the spa's menu, read-only (US1; editing comes in S4). */
export function PriceListPage() {
  const [categories, services] = useQueries({ queries: [categoriesQuery, servicesQuery] });

  if (categories.isPending || services.isPending) {
    return <p role="status" className="text-text-muted">Đang tải bảng giá…</p>;
  }
  if (categories.isError || services.isError) {
    return (
      <p role="alert" className="rounded-[10px] bg-danger-soft p-3 text-danger">
        {messageFor(categories.error ?? services.error)}
      </p>
    );
  }
  if (services.data.length === 0) {
    return <p className="text-text-muted">Chưa có dịch vụ nào trong bảng giá.</p>;
  }

  return (
    <section aria-labelledby="price-list-title" className="space-y-6">
      <h1 id="price-list-title" className="font-brand text-2xl text-primary">
        Bảng giá dịch vụ
      </h1>
      {categories.data.map((category) => {
        const items = services.data.filter((s) => s.category_id === category.id);
        if (items.length === 0) return null;
        return (
          <section key={category.id} aria-labelledby={`cat-${category.id}`} className="rounded-[14px] border border-border bg-surface p-4 shadow-[var(--shadow-1)]">
            <h2 id={`cat-${category.id}`} className="mb-3 text-lg font-semibold">
              {category.name}
            </h2>
            <ul className="divide-y divide-border">
              {items.map((service) => (
                <li key={service.id} className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 py-2">
                  <div className="min-w-0">
                    <p className="font-semibold">
                      {service.name}
                      {service.recommended && (
                        <span className="ml-2 text-sm text-accent-text" aria-label="Khuyên dùng">
                          ★ Khuyên dùng
                        </span>
                      )}
                    </p>
                    <p className="text-sm text-text-muted">
                      {describeDuration(service.duration_min)}
                      {service.description ? ` · ${service.description}` : ""}
                    </p>
                  </div>
                  <p className="flex gap-4 tabular-nums">
                    {describePrices(service.prices).map((line) => (
                      <span key={line.label ?? "all"}>
                        {line.label && <span className="mr-1 text-sm text-text-muted">{line.label}</span>}
                        <span className="font-semibold">{line.amount}</span>
                      </span>
                    ))}
                  </p>
                </li>
              ))}
            </ul>
          </section>
        );
      })}
    </section>
  );
}
