import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { adminApi, adminKeys, markAuditStale, type TenantRow } from "./api";
import { ReasonSheet } from "./ReasonSheet";

/**
 * ADR-0020: whether the spa's owners must sign in with a second factor. ON by default; OFF means
 * the password alone opens the owner's account. Switching ON signs the owners out; switching OFF
 * keeps their authenticator, so ON again works with the same app.
 */
export function OwnerMfaPanel({ tenant }: { tenant: TenantRow }) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const on = tenant.owner_mfa_required;

  return (
    <section className="panel" aria-labelledby="tenant-owner-mfa">
      <h2 id="tenant-owner-mfa" className="panel-title">
        Bảo mật đăng nhập
      </h2>
      <div className="flex flex-wrap items-center gap-3">
        <span className="min-w-48 font-medium">Bắt buộc xác thực 2 bước cho chủ spa</span>
        {on ? <span className="badge badge-ok">Đang bật</span> : <span className="badge badge-warn">Đang tắt</span>}
        <button
          type="button"
          className={on ? "btn btn-danger-outline ml-auto" : "btn btn-secondary ml-auto"}
          aria-label={`${on ? "Tắt" : "Bật"} xác thực 2 bước cho chủ spa`}
          onClick={() => {
            setOpen(true);
          }}
        >
          {on ? "Tắt" : "Bật"}
        </button>
      </div>
      {!on && <p className="callout">Chủ spa đang đăng nhập chỉ bằng mật khẩu, không cần mã xác thực.</p>}
      <ReasonSheet
        open={open}
        title={on ? "Tắt xác thực 2 bước cho chủ spa?" : "Bật xác thực 2 bước cho chủ spa?"}
        description={
          on
            ? "Từ lần đăng nhập sau, chủ spa chỉ cần mật khẩu. Ứng dụng xác thực đã cài được giữ để dùng lại khi bật."
            : "Chủ spa bị đăng xuất và phải nhập mã ở lần đăng nhập sau. Ai chưa cài ứng dụng xác thực sẽ được hướng dẫn cài."
        }
        confirm={on ? "Tắt xác thực 2 bước" : "Bật xác thực 2 bước"}
        danger={on}
        run={async (reason) => {
          const changed = await adminApi.setOwnerMfa(tenant.id, !on, reason);
          queryClient.setQueryData(adminKeys.tenant(tenant.id), changed.tenant);
          // Exact, as for a suspension: the prefix would refetch this spa's open panels too.
          await queryClient.invalidateQueries({ queryKey: adminKeys.tenants, exact: true });
          markAuditStale(queryClient);
        }}
        onClose={() => {
          setOpen(false);
        }}
      />
    </section>
  );
}
