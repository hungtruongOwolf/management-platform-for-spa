import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type SyntheticEvent } from "react";
import { BottomSheet } from "../components/BottomSheet";
import { adminApi, adminDateTime, adminKeys, type TenantFeature } from "./api";
import { adminFieldError, adminMessage } from "./messages";
import { StepUpCancelled, useStepUp } from "./StepUp";

/** Spec US5: a spa's feature switches. Off = hidden in the spa's app and refused by its API. */
export function FeaturesPanel({ tenantId }: { tenantId: string }) {
  const features = useQuery({ queryKey: adminKeys.features(tenantId), queryFn: () => adminApi.features(tenantId) });
  const [switching, setSwitching] = useState<TenantFeature | null>(null);

  return (
    <section className="panel" aria-labelledby="tenant-features">
      <h2 id="tenant-features" className="panel-title">
        Tính năng
      </h2>
      {features.isPending ? (
        <div role="status" aria-label="Đang tải" className="skeleton h-24" />
      ) : features.isError ? (
        <p role="alert" className="alert">
          {adminMessage(features.error)}
        </p>
      ) : (
        <ul className="grid gap-2">
          {features.data.map((f) => (
            <li key={f.key} className="flex flex-wrap items-center gap-3">
              <span className="min-w-48 font-medium">{f.name}</span>
              {f.enabled ? <span className="badge badge-ok">Đang bật</span> : <span className="badge badge-muted">Đang tắt</span>}
              <span className="text-xs text-text-muted">
                {f.updated_at ? `Đổi lúc ${adminDateTime(f.updated_at)}` : "Mặc định"}
              </span>
              <button
                type="button"
                className={f.enabled ? "btn btn-danger-outline ml-auto" : "btn btn-secondary ml-auto"}
                aria-label={`${f.enabled ? "Tắt" : "Bật"} ${f.name}`}
                onClick={() => {
                  setSwitching(f);
                }}
              >
                {f.enabled ? "Tắt" : "Bật"}
              </button>
            </li>
          ))}
        </ul>
      )}
      <SwitchSheet
        tenantId={tenantId}
        feature={switching}
        onClose={() => {
          setSwitching(null);
        }}
      />
    </section>
  );
}

function SwitchSheet({
  tenantId,
  feature,
  onClose,
}: {
  tenantId: string;
  feature: TenantFeature | null;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const stepUp = useStepUp();
  const [reason, setReason] = useState("");
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const turningOff = feature?.enabled ?? false;

  const submit = async (e: SyntheticEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!feature || busy) return;
    setBusy(true);
    setError(null);
    try {
      await stepUp(() => adminApi.setFeature(tenantId, feature.key, !feature.enabled, reason.trim()));
      await queryClient.invalidateQueries({ queryKey: adminKeys.features(tenantId) });
      setReason("");
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
      open={feature !== null}
      onClose={onClose}
      title={feature ? `${turningOff ? "Tắt" : "Bật"} ${feature.name}?` : ""}
      description={
        turningOff
          ? "Mọi người của spa không thấy và không dùng được tính năng này từ thao tác kế tiếp. Dữ liệu giữ nguyên."
          : "Tính năng hiện lại cho mọi người của spa từ thao tác kế tiếp."
      }
      footer={
        <div className="button-row">
          <button type="button" className="btn btn-ghost" onClick={onClose}>
            Hủy
          </button>
          <button
            type="submit"
            form="feature-switch-form"
            className={turningOff ? "btn btn-danger" : "btn btn-primary"}
            aria-busy={busy || undefined}
          >
            {turningOff ? "Tắt tính năng" : "Bật tính năng"}
          </button>
        </div>
      }
    >
      <form id="feature-switch-form" noValidate className="grid gap-3" onSubmit={(e) => void submit(e)}>
        <div className="field">
          <label htmlFor="feature-reason">Lý do</label>
          <textarea
            id="feature-reason"
            className="input"
            value={reason}
            onChange={(e) => {
              setReason(e.target.value);
            }}
            aria-invalid={reasonError ? true : undefined}
            aria-describedby={reasonError ? "feature-reason-error" : undefined}
          />
          {reasonError && (
            <p id="feature-reason-error" className="field-error">
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
