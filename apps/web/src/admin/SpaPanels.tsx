import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { adminApi, adminDateTime, adminKeys, markAuditStale } from "./api";
import { adminMessage } from "./messages";
import { RoleList } from "./RolesPage";

/** One spa's system roles: follow the template, or grants of its own (spec US7). */
export function SpaRolesPanel({ tenantId }: { tenantId: string }) {
  const queryClient = useQueryClient();
  const roles = useQuery({ queryKey: adminKeys.roles(tenantId), queryFn: () => adminApi.roles(tenantId) });
  const refresh = () => {
    markAuditStale(queryClient);
    return queryClient.invalidateQueries({ queryKey: adminKeys.roles(tenantId) });
  };
  return (
    <section className="grid gap-3" aria-labelledby="spa-roles">
      <h2 id="spa-roles" className="section-heading">
        Vai trò của spa
      </h2>
      {roles.isPending ? (
        <div role="status" aria-label="Đang tải" className="skeleton h-24" />
      ) : roles.isError ? (
        <p role="alert" className="alert">
          {adminMessage(roles.error)}
        </p>
      ) : (
        <RoleList
          roles={roles.data}
          save={async (key, permissions, reason) => {
            await adminApi.setRole(tenantId, key, permissions, reason);
            await refresh();
          }}
          reset={async (key, reason) => {
            await adminApi.resetRole(tenantId, key, reason);
            await refresh();
          }}
        />
      )}
    </section>
  );
}

/**
 * The spa's own audit trail for support (spec US9 AC9.2): who did what, when — no details, no
 * reasons, no ids. Opening it is itself audited, so it loads only when asked.
 */
export function SpaAuditPanel({ tenantId }: { tenantId: string }) {
  const [open, setOpen] = useState(false);
  const audit = useInfiniteQuery({
    queryKey: adminKeys.spaAudit(tenantId),
    queryFn: ({ pageParam }) => adminApi.spaAudit(tenantId, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last) => last.next_before ?? undefined,
    enabled: open,
    // Never a background read: each one is audited. A write marks it stale (markAuditStale), and
    // it reloads when the panel is next opened.
    staleTime: Infinity,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  });
  return (
    <section className="panel" aria-labelledby="spa-audit">
      <div className="panel-head">
        <h2 id="spa-audit" className="panel-title">
          Nhật ký của spa
        </h2>
        {!open && (
          <button
            type="button"
            className="btn btn-secondary ml-auto"
            onClick={() => {
              setOpen(true);
            }}
          >
            Xem nhật ký
          </button>
        )}
      </div>
      <p className="text-sm text-text-muted">Chỉ thao tác, loại bản ghi, người làm và lúc làm. Mỗi lần xem được ghi vào nhật ký nền tảng.</p>
      {open &&
        (audit.isPending ? (
          <div role="status" aria-label="Đang tải" className="skeleton h-24" />
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
                  <th scope="col">Thao tác</th>
                  <th scope="col">Loại</th>
                  <th scope="col">Người làm</th>
                </tr>
              </thead>
              <tbody>
                {audit.data.pages.flatMap((page) =>
                  page.items.map((item) => (
                    <tr key={item.id}>
                      <td>{adminDateTime(item.at)}</td>
                      <td className="font-mono text-xs">{item.action}</td>
                      <td>{item.entity_type}</td>
                      <td>
                        {item.actor_name ?? "—"}
                        {item.actor_kind === "platform_admin" && <span className="badge badge-gold ml-1">Vận hành</span>}
                        {item.actor_kind === "system" && <span className="badge badge-muted ml-1">Hệ thống</span>}
                      </td>
                    </tr>
                  )),
                )}
              </tbody>
            </table>
            {audit.hasNextPage && (
              <button type="button" className="btn btn-secondary" onClick={() => void audit.fetchNextPage()}>
                Xem thêm
              </button>
            )}
          </div>
        ))}
    </section>
  );
}
