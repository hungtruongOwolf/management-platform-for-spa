import type { Service, ServiceCategory } from "@vpyd/api-client";
import {
  useMutation,
  useQueries,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useId, useState } from "react";
import { Icon } from "../../app/Icon";
import { PageHeader } from "../../app/PageHeader";
import { fieldMessage, messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { ErrorState } from "../reports/ui";
import { categoriesQuery, createCategory, servicesQuery } from "./api";
import { describeDuration, describePrices } from "./prices";
import { ServiceForm } from "./ServiceForm";
import "../customers/khach.css";

type Editing =
  | { kind: "service"; service: Service }
  | { kind: "new-service"; categoryId: string }
  | null;

function ServiceRow({
  service,
  categories,
  canManage,
  editing,
  onEdit,
  onDone,
}: {
  service: Service;
  categories: ServiceCategory[];
  canManage: boolean;
  editing: boolean;
  onEdit: () => void;
  onDone: () => void;
}) {
  return (
    <li className={service.active ? undefined : "is-muted"}>
      <div className="price-row">
        <div className="min-w-0">
          <p className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className="row-title">{service.name}</span>
            {service.recommended && <span className="badge badge-gold">★ Khuyên dùng</span>}
            {!service.active && <span className="badge badge-muted">Ngừng bán</span>}
          </p>
          <p className="row-meta">
            {describeDuration(service.duration_min)}
            {service.description ? ` · ${service.description}` : ""}
          </p>
        </div>
        <p className="price-amounts">
          {describePrices(service.prices).map((line) => (
            <span key={line.label ?? "all"}>
              {line.label && <span className="price-label">{line.label}</span>}
              <span className="price-value">{line.amount}</span>
            </span>
          ))}
        </p>
        {canManage && (
          <span className="row-side">
            <button type="button" onClick={onEdit} aria-label={`Sửa ${service.name}`} aria-expanded={editing} className="btn btn-ghost">
              Sửa
            </button>
          </span>
        )}
      </div>
      {editing && <ServiceForm categories={categories} service={service} defaultCategoryId={service.category_id} onDone={onDone} />}
    </li>
  );
}

/** /bang-gia — the spa's menu. Owners (catalog.manage) can add and edit; everyone else reads. */
export function PriceListPage() {
  const { data: me } = useSuspenseQuery(meQuery);
  const canManage = can(me, "catalog.manage");
  const [categories, services] = useQueries({
    queries: [categoriesQuery, servicesQuery(canManage)],
  });
  const [editing, setEditing] = useState<Editing>(null);
  const done = () => {
    setEditing(null);
  };

  const header = (
    <PageHeader title="Bảng giá dịch vụ" titleId="price-list-title">
      <Link to="/bang-gia/goi" className="btn btn-secondary">
        Mẫu gói liệu trình
      </Link>
    </PageHeader>
  );

  if (categories.isPending || services.isPending) {
    return (
      <section aria-labelledby="price-list-title" className="khach-page">
        {header}
        <div role="status" aria-label="Đang tải bảng giá" className="grid gap-4">
          {[0, 1].map((i) => (
            <div key={i} className="panel">
              <div className="skeleton h-6 w-48" />
              <div className="skeleton h-12" />
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
      <section aria-labelledby="price-list-title" className="khach-page">
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
    <section aria-labelledby="price-list-title" className="khach-page">
      {header}
      {services.data.length === 0 && !canManage && (
        <p className="khach-empty">Chưa có dịch vụ nào trong bảng giá. Chủ spa thêm dịch vụ ở màn này.</p>
      )}

      {categories.data.map((category) => {
        const items = services.data.filter((s) => s.category_id === category.id);
        if (items.length === 0 && !canManage) return null;
        const selling = items.filter((s) => s.active);
        const stopped = items.filter((s) => !s.active);
        const row = (service: Service) => (
          <ServiceRow
            key={service.id}
            service={service}
            categories={categories.data}
            canManage={canManage}
            editing={editing?.kind === "service" && editing.service.id === service.id}
            onEdit={() => {
              setEditing(editing?.kind === "service" && editing.service.id === service.id ? null : { kind: "service", service });
            }}
            onDone={done}
          />
        );
        const adding = editing?.kind === "new-service" && editing.categoryId === category.id;
        return (
          <section key={category.id} aria-labelledby={`cat-${category.id}`} className="panel">
            <div className="panel-head">
              <h2 id={`cat-${category.id}`} className="panel-title">
                {category.name}
              </h2>
              {canManage && !adding && (
                <button
                  type="button"
                  onClick={() => {
                    setEditing({ kind: "new-service", categoryId: category.id });
                  }}
                  className="btn btn-secondary"
                >
                  <Icon name="plus" className="size-4" /> Thêm dịch vụ
                </button>
              )}
            </div>
            {adding && <ServiceForm categories={categories.data} defaultCategoryId={category.id} onDone={done} />}
            {items.length === 0 && !adding && <p className="khach-empty">Nhóm này chưa có dịch vụ. Bấm “Thêm dịch vụ” để thêm dịch vụ đầu tiên.</p>}
            {selling.length > 0 && (
              <ul className="row-list" aria-label={`Dịch vụ ${category.name}`}>
                {selling.map(row)}
              </ul>
            )}
            {stopped.length > 0 && (
              <details className="closed-group">
                <summary>
                  <Icon name="chevron-right" className="size-4" /> Ngừng bán ({stopped.length})
                </summary>
                <ul className="row-list" aria-label={`Dịch vụ ngừng bán ${category.name}`}>
                  {stopped.map(row)}
                </ul>
              </details>
            )}
          </section>
        );
      })}
      {canManage && <NewCategory />}
    </section>
  );
}

function NewCategory() {
  const queryClient = useQueryClient();
  const id = useId();
  const [name, setName] = useState("");
  const [touched, setTouched] = useState(false);
  const mutation = useMutation({
    mutationFn: () => createCategory({ name: name.trim() }),
    onSuccess: async () => {
      setName("");
      setTouched(false);
      await queryClient.invalidateQueries({ queryKey: ["catalog"] });
    },
  });
  const problem = (touched && name.trim() === "" ? "Nhập tên nhóm." : undefined) ?? fieldMessage(mutation.error, "name");
  return (
    <form
      noValidate
      onSubmit={(e) => {
        e.preventDefault();
        setTouched(true);
        if (name.trim() && !mutation.isPending) mutation.mutate();
      }}
      className="panel"
      aria-label="Thêm nhóm dịch vụ"
    >
      <h2 className="panel-title">Thêm nhóm dịch vụ</h2>
      <div className="field">
        <label htmlFor={`${id}-name`}>Tên nhóm dịch vụ mới</label>
        <div className="flex flex-wrap items-start gap-2">
          <input
            id={`${id}-name`}
            value={name}
            placeholder="Ví dụ: Chăm sóc tay chân"
            aria-invalid={problem ? true : undefined}
            aria-describedby={problem ? `${id}-error` : `${id}-hint`}
            onChange={(e) => {
              setName(e.target.value);
            }}
            className="input max-w-sm min-w-0 flex-1"
          />
          <button type="submit" aria-busy={mutation.isPending || undefined} className="btn btn-secondary">
            <Icon name="plus" className="size-4" /> Thêm nhóm
          </button>
        </div>
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
      {mutation.isError && !fieldMessage(mutation.error, "name") && (
        <p role="alert" className="form-error">
          {messageFor(mutation.error)}
        </p>
      )}
    </form>
  );
}
