import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type InputHTMLAttributes, type SyntheticEvent } from "react";
import { BottomSheet } from "../components/BottomSheet";
import { adminApi, adminDateTime, adminKeys, markAuditStale, NAME_MAX, type CreatedTenant, type TenantRow } from "./api";
import { adminFieldError, adminMessage } from "./messages";
import { AdminLink } from "./nav";
import { StepUpCancelled, useStepUp } from "./StepUp";

export function StatusBadge({ status }: { status: TenantRow["status"] }) {
  return status === "active" ? (
    <span className="badge badge-ok">Đang hoạt động</span>
  ) : (
    <span className="badge badge-danger">Tạm ngưng</span>
  );
}

const number = new Intl.NumberFormat("vi-VN");

/** Spec US2: every spa with status and counts — never its customers or money. */
export function TenantsPage() {
  const tenants = useQuery({ queryKey: adminKeys.tenants, queryFn: adminApi.tenants });
  const [creating, setCreating] = useState(false);
  const [created, setCreated] = useState<CreatedTenant | null>(null);

  return (
    <div className="grid content-start gap-4">
      <header className="page-header">
        <div className="page-header-main">
          <h1 className="page-title">Spa</h1>
        </div>
        <div className="page-header-actions">
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => {
              setCreating(true);
            }}
          >
            Tạo spa
          </button>
        </div>
      </header>

      <section className="panel" aria-label="Danh sách spa">
        {tenants.isPending ? (
          <div role="status" aria-label="Đang tải" className="grid gap-2">
            <div className="skeleton h-8" />
            <div className="skeleton h-8" />
          </div>
        ) : tenants.isError ? (
          <div className="grid gap-2">
            <p role="alert" className="alert">
              {adminMessage(tenants.error)}
            </p>
            <button type="button" className="btn btn-secondary" onClick={() => void tenants.refetch()}>
              Thử lại
            </button>
          </div>
        ) : tenants.data.length === 0 ? (
          <p className="empty-note">Chưa có spa nào. Bấm "Tạo spa" để mở spa đầu tiên.</p>
        ) : (
          <div className="table-wrap">
            <table className="data-table">
              <thead>
                <tr>
                  <th scope="col">Spa</th>
                  <th scope="col">Trạng thái</th>
                  <th scope="col">Ngày tạo</th>
                  <th scope="col" className="text-right">
                    Nhân viên
                  </th>
                  <th scope="col" className="text-right">
                    Khách
                  </th>
                  <th scope="col" className="text-right">
                    Lượt 30 ngày
                  </th>
                  <th scope="col">Hoạt động gần nhất</th>
                </tr>
              </thead>
              <tbody>
                {tenants.data.map((t) => (
                  <tr key={t.id}>
                    <td>
                      <AdminLink to={`/admin/spa/${t.id}`} className="name-link">
                        {t.name}
                      </AdminLink>
                      <div className="text-xs text-text-muted">{t.slug}</div>
                    </td>
                    <td>
                      <StatusBadge status={t.status} />
                    </td>
                    <td>{adminDateTime(t.created_at)}</td>
                    <td className="text-right tabular">{number.format(t.staff_count)}</td>
                    <td className="text-right tabular">{number.format(t.customer_count)}</td>
                    <td className="text-right tabular">{number.format(t.visits_30d)}</td>
                    <td>{adminDateTime(t.last_activity_at)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <CreateTenantSheet
        open={creating}
        onClose={() => {
          setCreating(false);
        }}
        onCreated={(result) => {
          setCreating(false);
          setCreated(result);
        }}
      />
      <CreatedOwnerSheet
        created={created}
        onClose={() => {
          setCreated(null);
        }}
      />
    </div>
  );
}

interface Draft {
  name: string;
  slug: string;
  branch: string;
  ownerName: string;
  ownerPhone: string;
}

const EMPTY: Draft = { name: "", slug: "", branch: "", ownerName: "", ownerPhone: "" };

/** "Spa Hoa Sen" → "spa-hoa-sen": a suggestion the operator can still edit. */
export function suggestSlug(name: string): string {
  return name
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/đ/gi, "d")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40)
    .replace(/-+$/g, "");
}

function CreateTenantSheet({
  open,
  onClose,
  onCreated,
}: {
  open: boolean;
  onClose: () => void;
  onCreated: (created: CreatedTenant) => void;
}) {
  const queryClient = useQueryClient();
  const stepUp = useStepUp();
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [slugTouched, setSlugTouched] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);

  const set = (field: keyof Draft, value: string) => {
    setDraft((d) => {
      const next = { ...d, [field]: value };
      if (field === "name" && !slugTouched) next.slug = suggestSlug(value);
      return next;
    });
  };

  const submit = async (e: SyntheticEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const created = await stepUp(() =>
        adminApi.createTenant({
          name: draft.name.trim(),
          slug: draft.slug.trim(),
          branch_name: draft.branch.trim() || null,
          owner: { full_name: draft.ownerName.trim(), phone: draft.ownerPhone.trim() },
        }),
      );
      await queryClient.invalidateQueries({ queryKey: adminKeys.tenants, exact: true });
      markAuditStale(queryClient);
      setDraft(EMPTY);
      setSlugTouched(false);
      onCreated(created);
    } catch (err) {
      if (!(err instanceof StepUpCancelled)) setError(err);
    } finally {
      setBusy(false);
    }
  };

  const fieldError = (field: string) => adminFieldError(error, field);
  const general = error && !["name", "slug", "branch_name", "owner.phone", "owner.full_name"].some((f) => fieldError(f));

  const input = (id: string, label: string, field: keyof Draft, apiField: string, extra: InputHTMLAttributes<HTMLInputElement> = {}) => {
    const message = fieldError(apiField);
    return (
      <div className="field">
        <label htmlFor={id}>{label}</label>
        <input
          id={id}
          className="input"
          value={draft[field]}
          onChange={(e) => {
            if (field === "slug") setSlugTouched(true);
            set(field, e.target.value);
          }}
          aria-invalid={message ? true : undefined}
          aria-describedby={message ? `${id}-error` : undefined}
          {...extra}
        />
        {message && (
          <p id={`${id}-error`} className="field-error">
            {message}
          </p>
        )}
      </div>
    );
  };

  return (
    <BottomSheet
      open={open}
      onClose={onClose}
      title="Tạo spa"
      description="Spa mới, chi nhánh đầu tiên và tài khoản chủ spa với mật khẩu tạm."
      wide
      footer={
        <div className="button-row">
          <button type="button" className="btn btn-ghost" onClick={onClose}>
            Hủy
          </button>
          <button type="submit" form="create-tenant-form" className="btn btn-primary" aria-busy={busy || undefined}>
            Tạo spa
          </button>
        </div>
      }
    >
      <form id="create-tenant-form" noValidate className="grid gap-4 sm:grid-cols-2" onSubmit={(e) => void submit(e)}>
        {input("tenant-name", "Tên spa", "name", "name", { maxLength: NAME_MAX })}
        {input("tenant-slug", "Mã spa", "slug", "slug", { autoCapitalize: "none" })}
        {input("tenant-branch", "Chi nhánh đầu tiên (không bắt buộc)", "branch", "branch_name", {
          placeholder: "Cơ sở chính",
          maxLength: NAME_MAX,
        })}
        <span aria-hidden="true" />
        {input("owner-name", "Tên chủ spa", "ownerName", "owner.full_name", { maxLength: NAME_MAX })}
        {input("owner-phone", "Số điện thoại chủ spa", "ownerPhone", "owner.phone", {
          type: "tel",
          inputMode: "numeric",
        })}
        {general ? (
          <p role="alert" className="alert sm:col-span-2">
            {adminMessage(error)}
          </p>
        ) : null}
      </form>
    </BottomSheet>
  );
}

/** The owner's temporary password, shown this once (ADR-0016). */
function CreatedOwnerSheet({ created, onClose }: { created: CreatedTenant | null; onClose: () => void }) {
  const [copied, setCopied] = useState(false);
  const password = created?.owner.temporary_password ?? "";
  return (
    <BottomSheet
      open={created !== null}
      onClose={onClose}
      title="Đã tạo spa"
      description={created ? `${created.tenant.name} — chủ spa đăng nhập bằng ${created.owner.phone}.` : undefined}
      footer={
        <button type="button" className="btn btn-primary" onClick={onClose}>
          Tôi đã lưu mật khẩu
        </button>
      }
    >
      <div className="grid gap-3">
        <p className="callout">
          Mật khẩu tạm chỉ hiện <strong>một lần</strong>. Đưa tận tay cho chủ spa; lần đăng nhập đầu họ phải thiết lập
          xác thực hai bước và đặt mật khẩu mới.
        </p>
        <p className="text-2xl font-mono tabular" aria-label="Mật khẩu tạm">
          {password}
        </p>
        <button
          type="button"
          className="btn btn-secondary"
          onClick={() => {
            void navigator.clipboard
              .writeText(password)
              .then(() => {
                setCopied(true);
              })
              .catch(() => undefined);
          }}
        >
          {copied ? "Đã sao chép" : "Sao chép"}
        </button>
      </div>
    </BottomSheet>
  );
}
