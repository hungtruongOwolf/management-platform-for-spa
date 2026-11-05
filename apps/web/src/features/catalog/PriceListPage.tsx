import type { Service } from "@vpyd/api-client";
import {
  useMutation,
  useQueries,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { PageHeader } from "../../app/PageHeader";
import { messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { categoriesQuery, createCategory, servicesQuery } from "./api";
import { describeDuration, describePrices } from "./prices";
import { ServiceForm } from "./ServiceForm";

type Editing =
  | { kind: "service"; service: Service }
  | { kind: "new-service"; categoryId: string }
  | null;

/** /bang-gia — the spa's menu. Owners (catalog.manage) can add and edit; everyone else reads. */
export function PriceListPage() {
  const { data: me } = useSuspenseQuery(meQuery);
  const canManage = can(me, "catalog.manage");
  const [categories, services] = useQueries({
    queries: [categoriesQuery, servicesQuery(canManage)],
  });
  const [editing, setEditing] = useState<Editing>(null);

  if (categories.isPending || services.isPending) {
    return (
      <p role="status" className="text-text-muted">
        Đang tải bảng giá…
      </p>
    );
  }
  if (categories.isError || services.isError) {
    return (
      <p role="alert" className="rounded-[10px] bg-danger-soft p-3 text-danger">
        {messageFor(categories.error ?? services.error)}
      </p>
    );
  }

  return (
    <section aria-labelledby="price-list-title" className="grid content-start gap-6">
      <PageHeader eyebrow="Danh mục" title="Bảng giá dịch vụ" titleId="price-list-title">
        <Link to="/bang-gia/goi" className="btn btn-secondary">
          Gói liệu trình
        </Link>
      </PageHeader>
      {services.data.length === 0 && !canManage && (
        <p className="text-text-muted">Chưa có dịch vụ nào trong bảng giá.</p>
      )}

      {categories.data.map((category) => {
        const items = services.data.filter(
          (s) => s.category_id === category.id,
        );
        if (items.length === 0 && !canManage) return null;
        return (
          <section
            key={category.id}
            aria-labelledby={`cat-${category.id}`}
            className="rounded-[14px] border border-border bg-surface p-4 shadow-card"
          >
            <div className="mb-3 flex items-center justify-between gap-2">
              <h2 id={`cat-${category.id}`} className="text-lg font-semibold">
                {category.name}
              </h2>
              {canManage && (
                <button
                  type="button"
                  onClick={() => {
                    setEditing({
                      kind: "new-service",
                      categoryId: category.id,
                    });
                  }}
                  className="min-h-11 rounded-[10px] border border-border px-3 text-sm font-medium hover:bg-surface-2"
                >
                  + Thêm dịch vụ
                </button>
              )}
            </div>
            {editing?.kind === "new-service" &&
              editing.categoryId === category.id && (
                <ServiceForm
                  categories={categories.data}
                  defaultCategoryId={category.id}
                  onDone={() => {
                    setEditing(null);
                  }}
                />
              )}
            <ul className="divide-y divide-border">
              {items.map((service) => (
                <li key={service.id} className="py-2">
                  <div
                    className={`flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 ${service.active ? "" : "text-text-muted"}`}
                  >
                    <div className="min-w-0">
                      <p className="font-semibold">
                        {service.name}
                        {service.recommended && (
                          <span
                            className="ml-2 text-sm text-accent-text"
                            aria-label="Khuyên dùng"
                          >
                            ★ Khuyên dùng
                          </span>
                        )}
                        {!service.active && (
                          <span className="ml-2 rounded-full bg-surface-2 px-2 text-xs text-text-muted">
                            Ngừng bán
                          </span>
                        )}
                      </p>
                      <p className="text-sm text-text-muted">
                        {describeDuration(service.duration_min)}
                        {service.description ? ` · ${service.description}` : ""}
                      </p>
                    </div>
                    <p className="flex flex-wrap items-center gap-x-4 gap-y-1 tabular-nums">
                      {describePrices(service.prices).map((line) => (
                        <span key={line.label ?? "all"}>
                          {line.label && (
                            <span className="mr-1 text-sm text-text-muted">
                              {line.label}
                            </span>
                          )}
                          <span className="font-semibold">{line.amount}</span>
                        </span>
                      ))}
                      {canManage && (
                        <button
                          type="button"
                          onClick={() => {
                            setEditing({ kind: "service", service });
                          }}
                          aria-label={`Sửa ${service.name}`}
                          className="min-h-11 rounded-[10px] border border-border px-3 text-sm hover:bg-surface-2"
                        >
                          Sửa
                        </button>
                      )}
                    </p>
                  </div>
                  {editing?.kind === "service" &&
                    editing.service.id === service.id && (
                      <ServiceForm
                        categories={categories.data}
                        service={service}
                        defaultCategoryId={category.id}
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
      })}
      {canManage && <NewCategory />}
    </section>
  );
}

function NewCategory() {
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const mutation = useMutation({
    mutationFn: () => createCategory({ name }),
    onSuccess: async () => {
      setName("");
      await queryClient.invalidateQueries({ queryKey: ["catalog"] });
    },
  });
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (name.trim()) mutation.mutate();
      }}
      className="flex flex-wrap items-end gap-2"
      aria-label="Thêm nhóm dịch vụ"
    >
      <label className="block">
        <span className="text-sm font-semibold">Nhóm dịch vụ mới</span>
        <input
          value={name}
          onChange={(e) => {
            setName(e.target.value);
          }}
          className="min-h-11 w-64 rounded-[6px] border border-border bg-surface px-3"
        />
      </label>
      <button
        type="submit"
        disabled={mutation.isPending}
        className="min-h-11 rounded-[10px] border border-border px-4 font-medium hover:bg-surface-2"
      >
        + Thêm nhóm
      </button>
      {mutation.isError && (
        <span role="alert" className="text-sm text-danger">
          {messageFor(mutation.error)}
        </span>
      )}
    </form>
  );
}
