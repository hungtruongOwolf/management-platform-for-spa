import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { adminApi, adminKeys, type PermissionInfo, type RoleGrants } from "./api";
import { adminMessage } from "./messages";
import { ReasonSheet } from "./ReasonSheet";

/** Areas of the catalogue, in the order the spa app shows them. */
const GROUPS: Record<string, string> = {
  catalog: "Danh mục",
  customer: "Khách hàng",
  package: "Gói liệu trình",
  sale: "Bán hàng",
  payment: "Thanh toán",
  visit: "Lượt khách",
  appointment: "Lịch hẹn",
  record: "Hủy / sửa sổ",
  report: "Báo cáo",
  staff: "Nhân sự",
  attendance: "Chấm công",
  evaluation: "Đánh giá",
  payroll: "Bảng lương",
  opening_balance: "Sổ cũ",
  audit: "Nhật ký",
  support_grant: "Hỗ trợ",
  platform: "Nền tảng",
};

/** Whose roles the editor changes: the templates (new spas, spas that follow them) or one spa's. */
export type RoleScope = "template" | "spa";

/** Can this checkbox be changed for this role (the server enforces the same rules)? */
export function locked(role: string, p: PermissionInfo, scope: RoleScope = "template"): string | null {
  if (!p.grantable) return "Không cấp được";
  if (role === "technician" && p.not_for_technician) return "Không cấp cho kỹ thuật viên";
  // ADR-0022 D1: each spa's own switch, changed only by its panel; a spa role edit keeps it.
  if (scope === "spa" && p.spa_local) return "Đổi ở panel Lịch sử trên từng bản ghi";
  if (role === "owner" && p.owner_floor) return "Chủ spa luôn giữ";
  if (role !== "owner" && p.owner_only) return "Chỉ chủ spa";
  return null;
}

/** Checkboxes of the catalogue, grouped by area, for one role. */
export function PermissionEditor({
  role,
  scope,
  catalogue,
  value,
  onChange,
}: {
  role: string;
  scope: RoleScope;
  catalogue: PermissionInfo[];
  value: string[];
  onChange: (next: string[]) => void;
}) {
  const groups = [...new Set(catalogue.map((p) => p.group))];
  return (
    <div className="grid gap-3">
      {groups.map((group) => (
        <fieldset key={group} className="grid gap-1">
          <legend className="section-label">{GROUPS[group] ?? group}</legend>
          {catalogue
            .filter((p) => p.group === group)
            .map((p) => {
              const why = locked(role, p, scope);
              const checked = value.includes(p.key);
              return (
                <label key={p.key} className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={checked}
                    disabled={why !== null}
                    onChange={(e) => {
                      onChange(e.target.checked ? [...value, p.key] : value.filter((k) => k !== p.key));
                    }}
                  />
                  <span className="font-mono">{p.key}</span>
                  {why && <span className="text-xs text-text-muted">({why})</span>}
                  {!why && scope === "template" && p.spa_local && (
                    <span className="text-xs text-text-muted">(Chỉ áp cho spa tạo mới)</span>
                  )}
                  {p.feature && <span className="badge badge-muted">tính năng {p.feature}</span>}
                </label>
              );
            })}
        </fieldset>
      ))}
    </div>
  );
}

function RoleCard({
  role,
  catalogue,
  onEdit,
  extra,
}: {
  role: RoleGrants;
  catalogue: PermissionInfo[];
  onEdit: () => void;
  extra?: React.ReactNode;
}) {
  return (
    <article className="panel" aria-labelledby={`role-${role.key}`}>
      <div className="panel-head">
        <h2 id={`role-${role.key}`} className="panel-title">
          {role.name}
        </h2>
        {role.follows_template === true && <span className="badge badge-ok">Theo mẫu</span>}
        {role.follows_template === false && <span className="badge badge-warn">Riêng spa này</span>}
        <div className="ml-auto flex gap-2">
          {extra}
          <button type="button" className="btn btn-secondary" onClick={onEdit} aria-label={`Sửa quyền ${role.name}`}>
            Sửa quyền
          </button>
        </div>
      </div>
      <p className="text-sm text-text-muted">
        {role.permissions.length} / {catalogue.filter((p) => p.grantable).length} quyền
      </p>
      <p className="font-mono text-xs break-words">{role.permissions.join(", ")}</p>
    </article>
  );
}

/**
 * Roles of one target (the templates, or one spa's roles): cards + an edit sheet. `save` writes
 * a role's permissions; `reset` (spa roles only) goes back to the template.
 */
export function RoleList({
  roles,
  save,
  reset,
}: {
  roles: RoleGrants[];
  save: (key: string, permissions: string[], reason: string) => Promise<void>;
  reset?: (key: string, reason: string) => Promise<void>;
}) {
  const catalogue = useQuery({ queryKey: adminKeys.permissions, queryFn: adminApi.permissions });
  const [editing, setEditing] = useState<{ role: RoleGrants; value: string[] } | null>(null);
  const [resetting, setResetting] = useState<RoleGrants | null>(null);
  if (catalogue.isPending) return <div role="status" aria-label="Đang tải" className="skeleton h-40" />;
  if (catalogue.isError)
    return (
      <p role="alert" className="alert">
        {adminMessage(catalogue.error)}
      </p>
    );
  return (
    <div className="grid gap-3">
      {roles.map((role) => (
        <RoleCard
          key={role.key}
          role={role}
          catalogue={catalogue.data}
          onEdit={() => {
            setEditing({ role, value: role.permissions });
          }}
          extra={
            reset && role.follows_template === false ? (
              <button
                type="button"
                className="btn btn-ghost"
                onClick={() => {
                  setResetting(role);
                }}
              >
                Về mẫu
              </button>
            ) : null
          }
        />
      ))}
      <ReasonSheet
        open={editing !== null}
        title={editing ? `Quyền của ${editing.role.name}` : ""}
        description="Có hiệu lực ở thao tác kế tiếp của mọi người có vai trò này."
        confirm="Lưu quyền"
        run={(reason) => (editing ? save(editing.role.key, editing.value, reason) : Promise.resolve())}
        onClose={() => {
          setEditing(null);
        }}
      >
        {editing && (
          <PermissionEditor
            role={editing.role.key}
            scope={reset ? "spa" : "template"}
            catalogue={catalogue.data}
            value={editing.value}
            onChange={(value) => {
              setEditing({ ...editing, value });
            }}
          />
        )}
      </ReasonSheet>
      <ReasonSheet
        open={resetting !== null}
        title={resetting ? `Đưa ${resetting.name} về mẫu?` : ""}
        description="Vai trò này lấy lại quyền của mẫu và đổi theo mẫu từ nay."
        confirm="Về mẫu"
        run={(reason) => (resetting && reset ? reset(resetting.key, reason) : Promise.resolve())}
        onClose={() => {
          setResetting(null);
        }}
      />
    </div>
  );
}

/** "Mẫu vai trò" (spec US6–US7): the default grants of new spas and of spas that follow them. */
export function RolesPage() {
  const queryClient = useQueryClient();
  const templates = useQuery({ queryKey: adminKeys.templates, queryFn: adminApi.templates });
  return (
    <div className="grid content-start gap-4">
      <header className="page-header">
        <div className="page-header-main">
          <h1 className="page-title">Mẫu vai trò</h1>
        </div>
      </header>
      <p className="text-sm text-text-muted">
        Spa mới nhận đúng các quyền này. Sửa mẫu thì mọi spa đang "Theo mẫu" đổi theo ngay; spa có quyền riêng giữ nguyên.
      </p>
      {templates.isPending ? (
        <div role="status" aria-label="Đang tải" className="skeleton h-40" />
      ) : templates.isError ? (
        <p role="alert" className="alert">
          {adminMessage(templates.error)}
        </p>
      ) : (
        <RoleList
          roles={templates.data}
          save={async (key, permissions, reason) => {
            await adminApi.setTemplate(key, permissions, reason);
            await queryClient.invalidateQueries({ queryKey: ["admin"] });
          }}
        />
      )}
    </div>
  );
}
