import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { adminApi, adminDateTime, adminKeys } from "./api";
import { adminMessage } from "./messages";

/** Vietnamese names of the platform audit actions; unknown ones show their code. */
const ACTIONS: Record<string, string> = {
  "platform.auth.login": "Đăng nhập",
  "platform.auth.logout": "Đăng xuất",
  "platform.auth.reauth": "Xác nhận lại",
  "platform.auth.mfa_enrolled": "Bật xác thực hai bước",
  "platform.tenant.create": "Tạo spa",
  "platform.tenant.suspend": "Tạm ngưng spa",
  "platform.tenant.unsuspend": "Mở lại spa",
  "platform.feature.set": "Bật / tắt tính năng",
  "platform.role_template.set": "Sửa mẫu vai trò",
  "platform.role.set": "Sửa quyền vai trò của spa",
  "platform.role.reset": "Đưa vai trò của spa về mẫu",
  "platform.account.lock": "Khóa tài khoản",
  "platform.account.unlock": "Mở khóa tài khoản",
  "platform.account.reset_password": "Đặt lại mật khẩu",
  "platform.account.reset_mfa": "Đặt lại xác thực hai bước",
  "platform.account.revoke_sessions": "Đăng xuất mọi thiết bị",
  "platform.tenant_audit.read": "Xem nhật ký spa",
};

export function actionLabel(action: string): string {
  return ACTIONS[action] ?? action;
}

/** Spec US9: every operator action, newest first. */
export function AuditPage() {
  const tenants = useQuery({ queryKey: adminKeys.tenants, queryFn: adminApi.tenants });
  const audit = useInfiniteQuery({
    queryKey: adminKeys.audit,
    queryFn: ({ pageParam }) => adminApi.audit(pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last) => last.next_before ?? undefined,
  });
  const spaName = (id: string | null | undefined) =>
    id ? (tenants.data?.find((t) => t.id === id)?.name ?? id.slice(0, 8)) : "—";

  return (
    <div className="grid content-start gap-4">
      <header className="page-header">
        <div className="page-header-main">
          <h1 className="page-title">Nhật ký nền tảng</h1>
        </div>
      </header>
      <section className="panel" aria-label="Nhật ký nền tảng">
        {audit.isPending ? (
          <div role="status" aria-label="Đang tải" className="skeleton h-40" />
        ) : audit.isError ? (
          <p role="alert" className="alert">
            {adminMessage(audit.error)}
          </p>
        ) : (
          <div className="table-wrap">
            <table className="data-table">
              <thead>
                <tr>
                  <th scope="col">Lúc</th>
                  <th scope="col">Người vận hành</th>
                  <th scope="col">Thao tác</th>
                  <th scope="col">Spa</th>
                  <th scope="col">Lý do</th>
                </tr>
              </thead>
              <tbody>
                {audit.data.pages.flatMap((page) =>
                  page.items.map((item) => (
                    <tr key={item.id}>
                      <td>{adminDateTime(item.at)}</td>
                      <td>{item.operator_username}</td>
                      <td>{actionLabel(item.action)}</td>
                      <td>{spaName(item.tenant_id)}</td>
                      <td>{item.reason ?? "—"}</td>
                    </tr>
                  )),
                )}
              </tbody>
            </table>
            {audit.hasNextPage && (
              <button
                type="button"
                className="btn btn-secondary"
                aria-busy={audit.isFetchingNextPage || undefined}
                onClick={() => void audit.fetchNextPage()}
              >
                Xem thêm
              </button>
            )}
          </div>
        )}
      </section>
    </div>
  );
}
