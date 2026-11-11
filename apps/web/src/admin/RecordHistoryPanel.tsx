import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { ApiError } from "../lib/api";
import { adminApi, adminKeys, markAuditStale } from "./api";
import { adminMessage } from "./messages";
import { ReasonSheet } from "./ReasonSheet";

/** The spa-local permission behind the panel (ADR-0022 D1, D2). */
export const RECORD_HISTORY = "record.history.read";

const ROLES = [
  { key: "owner", name: "Chủ spa" },
  { key: "manager", name: "Quản lý" },
] as const;

type Switching = { key: (typeof ROLES)[number]["key"]; name: string; shown: boolean };

/**
 * ADR-0022: "Ghi bởi … · đã sửa … lần" and each record's history, shown or hidden per role of
 * this spa. Shown to the owner and hidden from the manager by default; never a technician's. The
 * spa's own switch: editing a role template does not change it.
 */
export function RecordHistoryPanel({ tenantId }: { tenantId: string }) {
  const queryClient = useQueryClient();
  const roles = useQuery({ queryKey: adminKeys.roles(tenantId), queryFn: () => adminApi.roles(tenantId) });
  const [switching, setSwitching] = useState<Switching | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const description = (s: Switching | null) => {
    if (!s) return "";
    if (s.shown) return `Từ thao tác kế tiếp, ${s.name} không thấy ai ghi, ai sửa nữa. Dữ liệu giữ nguyên.`;
    return s.key === "manager"
      ? "Từ thao tác kế tiếp, Quản lý thấy ai ghi, ai sửa trên những bản ghi họ mở được. Số điện thoại cũ của khách không hiện."
      : "Từ thao tác kế tiếp, Chủ spa thấy ai ghi, ai sửa trên những bản ghi họ mở được.";
  };

  return (
    <section className="panel" aria-labelledby="tenant-record-history">
      <h2 id="tenant-record-history" className="panel-title">
        Lịch sử trên từng bản ghi
      </h2>
      <p className="text-sm text-text-muted">
        Dòng “Ghi bởi … · đã sửa … lần” và lịch sử thay đổi trên lượt, hóa đơn, gói, hồ sơ khách… Kỹ thuật viên không bao giờ
        thấy. Công tắc riêng của spa này: sửa mẫu vai trò không đổi nó.
      </p>
      {notice && (
        <p role="status" className="callout">
          {notice}
        </p>
      )}
      {roles.isPending ? (
        <div role="status" aria-label="Đang tải" className="skeleton h-16" />
      ) : roles.isError ? (
        <p role="alert" className="alert">
          {adminMessage(roles.error)}
        </p>
      ) : (
        <ul className="grid gap-2">
          {ROLES.map((r) => {
            const shown = roles.data.find((x) => x.key === r.key)?.permissions.includes(RECORD_HISTORY) ?? false;
            const label = shown ? `Ẩn với ${r.name}` : `Hiện cho ${r.name}`;
            return (
              <li key={r.key} className="flex flex-wrap items-center gap-3">
                <span className="min-w-48 font-medium">{r.name}</span>
                {shown ? <span className="badge badge-ok">Đang hiện</span> : <span className="badge badge-muted">Đang ẩn</span>}
                <button
                  type="button"
                  className="btn btn-secondary ml-auto"
                  aria-label={`${label}: lịch sử trên từng bản ghi`}
                  onClick={() => {
                    setNotice(null);
                    setSwitching({ key: r.key, name: r.name, shown });
                  }}
                >
                  {label}
                </button>
              </li>
            );
          })}
        </ul>
      )}
      <ReasonSheet
        open={switching !== null}
        title={
          switching ? (switching.shown ? `Ẩn lịch sử với ${switching.name}?` : `Hiện lịch sử cho ${switching.name}?`) : ""
        }
        description={description(switching)}
        confirm={switching?.shown ? "Ẩn lịch sử" : "Hiện lịch sử"}
        danger={switching?.shown ?? false}
        run={async (reason) => {
          if (!switching) return;
          try {
            const role = await adminApi.setRecordHistory(tenantId, switching.key, !switching.shown, reason);
            queryClient.setQueryData(adminKeys.roles(tenantId), (old: typeof roles.data) =>
              old?.map((x) => (x.key === role.key ? role : x)),
            );
          } catch (err) {
            // Someone changed it meanwhile: show the current state instead of an error.
            if (!(err instanceof ApiError && err.code === "role.record_history_unchanged")) throw err;
            setNotice("Trạng thái đã đổi ở nơi khác, đã tải lại.");
          }
          await queryClient.invalidateQueries({ queryKey: adminKeys.roles(tenantId) });
          markAuditStale(queryClient);
        }}
        onClose={() => {
          setSwitching(null);
        }}
      />
    </section>
  );
}
