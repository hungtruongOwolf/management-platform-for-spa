import type { Service, ServiceCategory } from "@vpyd/api-client";
import {
  useMutation,
  useQueries,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useEffect, useId, useRef, useState } from "react";
import { Icon } from "../../app/Icon";
import { PageHeader } from "../../app/PageHeader";
import { fieldMessage, messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { usePreloadRecordHistory } from "../record-history/lazy";
import { canTrace } from "../record-history/query";
import { ErrorState } from "../reports/ui";
import { categoriesQuery, createCategory, servicesQuery } from "./api";
import { CategoryForm } from "./CategoryForm";
import { describeDuration, describePrices } from "./prices";
import { ServiceForm } from "./ServiceForm";
import "../customers/khach.css";
import { CATALOG_FIELDS } from "./messages";

type Editing =
  | { kind: "service"; service: Service }
  | { kind: "category"; category: ServiceCategory }
  | { kind: "new-service"; categoryId: string }
  | { kind: "new-category" }
  | null;

/**
 * /bang-gia — the spa's menu. Owners (catalog.manage) can add and edit; everyone else reads.
 * Round-5 layout (EM 2026-11-06 kept it): one card per group, services divided by hairlines.
 * "Thêm nhóm dịch vụ" sits in the page header, where "Mẫu gói liệu trình" has "Tạo mẫu gói", and
 * opens its form at the top of the page; "Sửa nhóm" on a group's heading opens its rename form
 * under the heading (spec record-history K39, K48), where a group with no service can also be
 * deleted (K68).
 */
export function PriceListPage() {
  const { data: me } = useSuspenseQuery(meQuery);
  const canManage = can(me, "catalog.manage");
  // "Ai ghi, ai sửa" at the end of a service's and a group's edit form (ADR-0022 wave 3), never on the list.
  const traced = canTrace(me, "catalog");
  usePreloadRecordHistory(traced);
  const [categories, services] = useQueries({
    queries: [categoriesQuery, servicesQuery(canManage)],
  });
  const [editing, setEditing] = useState<Editing>(null);
  const done = () => {
    setEditing(null);
  };
  // After "Xóa nhóm" (K68/K71) the button that had focus is gone: focus the next group's heading,
  // or the page's title when the deleted group was the last one, once the form has closed (the
  // effect after that render). `categories` is the list the form was opened from, the deleted
  // group still in it. A ref and an effect, not `flushSync`: `react-dom` exported to this chunk
  // costs startup bytes on every path.
  const refocus = useRef<string | null>(null);
  useEffect(() => {
    if (refocus.current === null) return;
    focusHeading(refocus.current);
    refocus.current = null;
  });
  const deleted = (id: string) => {
    const order = categories.data?.map((c) => c.id) ?? [];
    const next = order[order.indexOf(id) + 1];
    refocus.current = next ? `cat-${next}` : "price-list-title";
    setEditing(null);
  };

  const header = (
    <PageHeader title="Bảng giá dịch vụ" titleId="price-list-title">
      <Link to="/bang-gia/goi" className="btn btn-secondary">
        Mẫu gói liệu trình
      </Link>
      {canManage && editing?.kind !== "new-category" && (
        <button
          type="button"
          onClick={() => {
            setEditing({ kind: "new-category" });
          }}
          className="btn btn-primary"
        >
          <Icon name="plus" className="size-4" /> Thêm nhóm dịch vụ
        </button>
      )}
    </PageHeader>
  );

  if (categories.isPending || services.isPending) {
    return (
      <section aria-labelledby="price-list-title" className="grid content-start gap-6">
        {header}
        <div role="status" aria-label="Đang tải bảng giá" className="grid content-start gap-6">
          {[0, 1].map((i) => (
            <div key={i} className="card grid content-start gap-3 p-4">
              <div className="skeleton h-6 w-48" />
              <div className="skeleton h-12" />
              <div className="skeleton h-12" />
            </div>
          ))}
        </div>
      </section>
    );
  }
  if (categories.isError || services.isError) {
    return (
      <section aria-labelledby="price-list-title" className="grid content-start gap-6">
        {header}
        <ErrorState
          error={categories.error ?? services.error}
          onRetry={() => {
            void categories.refetch();
            void services.refetch();
          }}
        />
      </section>
    );
  }

  return (
    <section aria-labelledby="price-list-title" className="grid content-start gap-6">
      {header}
      {editing?.kind === "new-category" && <NewCategory onDone={done} />}
      {services.data.length === 0 && !canManage && (
        <p className="empty-block">Chưa có dịch vụ nào trong bảng giá.</p>
      )}

      {categories.data.map((category) => {
        const items = services.data.filter((s) => s.category_id === category.id);
        if (items.length === 0 && !canManage) return null;
        const adding = editing?.kind === "new-service" && editing.categoryId === category.id;
        const renaming = editing?.kind === "category" && editing.category.id === category.id;
        return (
          <section key={category.id} aria-labelledby={`cat-${category.id}`} className="card p-4">
            <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
              <h2 id={`cat-${category.id}`} className="text-lg font-semibold">
                {category.name}
              </h2>
              {canManage && (
                <div className="flex flex-wrap gap-2">
                  <button
                    type="button"
                    onClick={() => {
                      setEditing(renaming ? null : { kind: "category", category });
                    }}
                    aria-label={`Sửa nhóm ${category.name}`}
                    aria-expanded={renaming}
                    className="btn btn-secondary"
                  >
                    Sửa nhóm
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setEditing(adding ? null : { kind: "new-service", categoryId: category.id });
                    }}
                    aria-expanded={adding}
                    className="btn btn-secondary"
                  >
                    <Icon name="plus" className="size-4" /> Thêm dịch vụ
                  </button>
                </div>
              )}
            </div>
            {renaming && (
              <CategoryForm
                category={category}
                onDone={done}
                traced={traced}
                // Every service counts, on sale or not: the owner's list has them all (K68).
                deletable={items.length === 0}
                onDeleted={() => {
                  deleted(category.id);
                }}
              />
            )}
            {adding && <ServiceForm categories={categories.data} defaultCategoryId={category.id} onDone={done} />}
            {items.length === 0 && !adding && (
              <p className="py-2 text-sm text-text-muted">Nhóm này chưa có dịch vụ. Bấm “Thêm dịch vụ” để thêm dịch vụ đầu tiên.</p>
            )}
            <ul className="divide-y divide-border">
              {items.map((service) => {
                const open = editing?.kind === "service" && editing.service.id === service.id;
                return (
                  <li key={service.id} className="py-2">
                    <div className={`flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 ${service.active ? "" : "text-text-muted"}`}>
                      <div className="min-w-0">
                        <p className="flex flex-wrap items-center gap-x-2 gap-y-1 font-semibold">
                          {service.name}
                          {service.recommended && <span className="badge badge-gold">★ Khuyên dùng</span>}
                          {!service.active && <span className="badge badge-muted">Ngừng bán</span>}
                        </p>
                        <p className="text-sm text-text-muted">
                          {describeDuration(service.duration_min)}
                          {service.description ? ` · ${service.description}` : ""}
                        </p>
                      </div>
                      <p className="catalog-price flex flex-wrap items-center gap-x-4 gap-y-1 tabular-nums">
                        {describePrices(service.prices).map((line) => (
                          <span key={line.label ?? "all"}>
                            {line.label && <span className="mr-1 text-sm text-text-muted">{line.label}</span>}
                            <span className="font-semibold">{line.amount}</span>
                          </span>
                        ))}
                        {canManage && (
                          <button
                            type="button"
                            onClick={() => {
                              setEditing(open ? null : { kind: "service", service });
                            }}
                            aria-label={`Sửa ${service.name}`}
                            aria-expanded={open}
                            className="btn btn-secondary"
                          >
                            Sửa
                          </button>
                        )}
                      </p>
                    </div>
                    {open && <ServiceForm categories={categories.data} service={service} defaultCategoryId={category.id} onDone={done} traced={traced} />}
                  </li>
                );
              })}
            </ul>
          </section>
        );
      })}
    </section>
  );
}

/** Moves focus to a heading of the page, focusable from script only (not a tab stop). */
function focusHeading(id: string) {
  const heading = document.getElementById(id);
  if (!heading) return;
  if (!heading.hasAttribute("tabindex")) heading.setAttribute("tabindex", "-1");
  heading.focus();
}

/** "Thêm nhóm dịch vụ": opened from the page header, at the top of the page. */
function NewCategory({ onDone }: { onDone: () => void }) {
  const queryClient = useQueryClient();
  const id = useId();
  const input = useRef<HTMLInputElement>(null);
  const [name, setName] = useState("");
  const [touched, setTouched] = useState(false);
  const mutation = useMutation({
    mutationFn: () => createCategory({ name: name.trim() }),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["catalog"] });
      onDone();
    },
  });
  useEffect(() => {
    input.current?.focus();
  }, []);
  const problem = (touched && name.trim() === "" ? "Nhập tên nhóm." : undefined) ?? fieldMessage(mutation.error, "name", CATALOG_FIELDS);
  return (
    <form
      noValidate
      onSubmit={(e) => {
        e.preventDefault();
        setTouched(true);
        if (name.trim() && !mutation.isPending) mutation.mutate();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") onDone();
      }}
      className="card grid content-start gap-3 p-4"
      aria-label="Thêm nhóm dịch vụ"
    >
      <div className="field">
        <label htmlFor={`${id}-name`}>Tên nhóm dịch vụ mới</label>
        <input
          id={`${id}-name`}
          ref={input}
          value={name}
          placeholder="Ví dụ: Chăm sóc tay chân"
          aria-invalid={problem ? true : undefined}
          aria-describedby={problem ? `${id}-error` : `${id}-hint`}
          onChange={(e) => {
            setName(e.target.value);
          }}
          className="input max-w-md"
        />
        {problem ? (
          <span id={`${id}-error`} className="field-error">
            {problem}
          </span>
        ) : (
          <span id={`${id}-hint`} className="field-hint">
            Nhóm mới hiện ở cuối bảng giá; thêm dịch vụ vào nhóm bằng “Thêm dịch vụ”.
          </span>
        )}
      </div>
      {mutation.isError && !fieldMessage(mutation.error, "name", CATALOG_FIELDS) && (
        <p role="alert" className="alert">
          {messageFor(mutation.error)}
        </p>
      )}
      <div className="button-row">
        <button type="submit" aria-busy={mutation.isPending || undefined} className="btn btn-primary">
          Lưu nhóm dịch vụ
        </button>
        <button type="button" onClick={onDone} className="btn btn-secondary">
          Hủy
        </button>
      </div>
    </form>
  );
}
