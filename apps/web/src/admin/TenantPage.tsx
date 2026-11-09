import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type SyntheticEvent } from "react";
import { BottomSheet } from "../components/BottomSheet";
import { adminApi, adminDateTime, adminKeys, markAuditStale, REASON_MAX, type TenantRow } from "./api";
import { FeaturesPanel } from "./FeaturesPanel";
import { OwnerMfaPanel } from "./OwnerMfaPanel";
import { SpaAuditPanel, SpaRolesPanel } from "./SpaPanels";
import { adminFieldError, adminMessage } from "./messages";
import { AdminLink } from "./nav";
import { StepUpCancelled, useStepUp } from "./StepUp";
import { useResetOnOpen } from "./useResetOnOpen";
import { StatusBadge } from "./TenantsPage";

/** One spa: its metadata and the suspend / reopen switch (spec US4). */
export function TenantPage({ id }: { id: string }) {
  const tenant = useQuery({ queryKey: adminKeys.tenant(id), queryFn: () => adminApi.tenant(id) });
  const [changing, setChanging] = useState(false);

  if (tenant.isPending) {
    return (
      <div role="status" aria-label="Đang tải" className="grid gap-3">
        <div className="skeleton h-8 w-64" />
        <div className="skeleton h-40" />
      </div>
    );
  }
  if (tenant.isError) {
    return (
      <div className="grid gap-3">
        <AdminLink to="/admin" className="back-link">
          ← Spa
        </AdminLink>
        <p role="alert" className="alert">
          {adminMessage(tenant.error)}
        </p>
      </div>
    );
  }
  const t = tenant.data;
  const suspended = t.status === "suspended";
  return (
    <div className="grid content-start gap-4">
      <AdminLink to="/admin" className="back-link">
        ← Spa
      </AdminLink>
      <header className="page-header">
        <div className="page-header-main">
          <h1 className="page-title">{t.name}</h1>
          <StatusBadge status={t.status} />
        </div>
        <div className="page-header-actions">
          <button
            type="button"
            className={suspended ? "btn btn-secondary" : "btn btn-danger-outline"}
            onClick={() => {
              setChanging(true);
            }}
          >
            {suspended ? "Mở lại spa" : "Tạm ngưng spa"}
          </button>
        </div>
      </header>
      <section className="panel" aria-labelledby="tenant-info">
        <h2 id="tenant-info" className="panel-title">
          Thông tin
        </h2>
        <dl className="detail-grid">
          <dt>Mã spa</dt>
          <dd>{t.slug}</dd>
          <dt>Ngày tạo</dt>
          <dd>{adminDateTime(t.created_at)}</dd>
          <dt>Chủ spa đang hoạt động</dt>
          <dd className="tabular">{t.owner_count}</dd>
          <dt>Nhân viên đang làm</dt>
          <dd className="tabular">{t.staff_count}</dd>
          <dt>Hồ sơ khách</dt>
          <dd className="tabular">{t.customer_count}</dd>
          <dt>Lượt khách 30 ngày</dt>
          <dd className="tabular">{t.visits_30d}</dd>
          <dt>Hoạt động gần nhất</dt>
          <dd>{adminDateTime(t.last_activity_at)}</dd>
        </dl>
        <p className="text-sm text-text-muted">
          Chỉ có số đếm: console không xem được khách, ghi chú hay tiền của spa.
        </p>
      </section>
      <OwnerMfaPanel tenant={t} />
      <FeaturesPanel tenantId={t.id} />
      <SpaRolesPanel tenantId={t.id} />
      <SpaAuditPanel tenantId={t.id} />
      <StatusSheet
        tenant={t}
        open={changing}
        onClose={() => {
          setChanging(false);
        }}
      />
    </div>
  );
}

function StatusSheet({ tenant, open, onClose }: { tenant: TenantRow; open: boolean; onClose: () => void }) {
  const queryClient = useQueryClient();
  const stepUp = useStepUp();
  const [reason, setReason] = useState("");
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const suspend = tenant.status === "active";
  useResetOnOpen(open, () => {
    setReason("");
    setError(null);
  });

  const submit = async (e: SyntheticEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const changed = await stepUp(() => adminApi.setStatus(tenant.id, suspend, reason.trim()));
      queryClient.setQueryData(adminKeys.tenant(tenant.id), changed.tenant);
      // Exact: the prefix would also refetch this spa's open panels, and each refetch of its
      // audit trail writes a `platform.tenant_audit.read` row.
      await queryClient.invalidateQueries({ queryKey: adminKeys.tenants, exact: true });
      markAuditStale(queryClient);
      onClose();
    } catch (err) {
      if (!(err instanceof StepUpCancelled)) setError(err);
    } finally {
      setBusy(false);
    }
  };

  const reasonError = adminFieldError(error, "reason");
  return (
    <BottomSheet
      open={open}
      onClose={onClose}
      title={suspend ? `Tạm ngưng ${tenant.name}?` : `Mở lại ${tenant.name}?`}
      description={
        suspend
          ? "Mọi người của spa bị đăng xuất và không đăng nhập được cho tới khi mở lại. Dữ liệu giữ nguyên."
          : "Người của spa đăng nhập lại được như trước."
      }
      footer={
        <div className="button-row">
          <button type="button" className="btn btn-ghost" onClick={onClose}>
            Hủy
          </button>
          <button
            type="submit"
            form="tenant-status-form"
            className={suspend ? "btn btn-danger" : "btn btn-primary"}
            aria-busy={busy || undefined}
          >
            {suspend ? "Tạm ngưng" : "Mở lại"}
          </button>
        </div>
      }
    >
      <form id="tenant-status-form" noValidate className="grid gap-3" onSubmit={(e) => void submit(e)}>
        <div className="field">
          <label htmlFor="status-reason">Lý do</label>
          <textarea
            id="status-reason"
            className="input"
            maxLength={REASON_MAX}
            value={reason}
            onChange={(e) => {
              setReason(e.target.value);
            }}
            aria-invalid={reasonError ? true : undefined}
            aria-describedby={reasonError ? "status-reason-error" : undefined}
          />
          {reasonError && (
            <p id="status-reason-error" className="field-error">
              {reasonError}
            </p>
          )}
        </div>
        {error && !reasonError ? (
          <p role="alert" className="alert">
            {adminMessage(error)}
          </p>
        ) : null}
      </form>
    </BottomSheet>
  );
}
