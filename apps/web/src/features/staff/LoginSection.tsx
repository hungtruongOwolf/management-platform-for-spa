import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState, type SyntheticEvent } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { ApiError } from "../../lib/api";
import { fieldMessage, messageFor } from "../../lib/errors";
import {
  LOGIN_ROLE_LABEL,
  createLogin,
  invalidateStaff,
  reauth,
  resetLoginPassword,
  setLoginRole,
  type LoginRole,
  type StaffProfile,
} from "./api";

const ROLES: LoginRole[] = ["technician", "manager"];

function needsReauth(error: unknown): boolean {
  return error instanceof ApiError && error.code === "auth.reauth_required";
}

/**
 * The owner's password (and authenticator code) again before a login action (ADR-0004 step-up,
 * ADR-0016). `onDone` retries the action that asked for it.
 */
export function ReauthDialog({ onDone, onClose }: { onDone: () => void; onClose: () => void }) {
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const confirm = useMutation({ mutationFn: () => reauth(password, code.trim()), onSuccess: onDone });
  const error = confirm.error;
  const message =
    error instanceof ApiError && error.status === 401
      ? "Mật khẩu hoặc mã xác thực không đúng."
      : error && fieldMessage(error, "code")
        ? "Nhập mã xác thực trong ứng dụng xác thực."
        : error
          ? messageFor(error)
          : null;
  const formId = "reauth-form";
  return (
    <BottomSheet
      open
      onClose={onClose}
      title="Xác nhận lại mật khẩu"
      description="Thao tác với tài khoản đăng nhập cần nhập lại mật khẩu của bạn."
      footer={
        <div className="flex flex-wrap gap-2">
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            Thôi
          </button>
          <button type="submit" form={formId} className="btn btn-primary" disabled={!password} aria-busy={confirm.isPending || undefined}>
            Xác nhận
          </button>
        </div>
      }
    >
      <form
        id={formId}
        className="grid gap-4"
        onSubmit={(e: SyntheticEvent) => {
          e.preventDefault();
          if (!confirm.isPending) confirm.mutate();
        }}
      >
        <label className="filter-field">
          Mật khẩu của bạn
          <input type="password" autoComplete="current-password" required value={password} onChange={(e) => { setPassword(e.target.value); }} />
        </label>
        <label className="filter-field">
          Mã xác thực (nếu tài khoản có)
          <input inputMode="numeric" autoComplete="one-time-code" maxLength={20} value={code} onChange={(e) => { setCode(e.target.value); }} />
        </label>
        {message && (
          <p role="alert" className="text-sm text-danger">
            {message}
          </p>
        )}
      </form>
    </BottomSheet>
  );
}

/**
 * A temporary password, shown this once (ADR-0016): copy it, hand it over, confirm. The owner
 * never sees it again; the person must set their own at the first sign-in.
 */
export function TemporaryPasswordDialog({ name, phone, password, onClose }: { name: string; phone: string | null; password: string; onClose: () => void }) {
  const [handed, setHanded] = useState(false);
  const [copied, setCopied] = useState(false);
  return (
    <BottomSheet
      open
      onClose={() => {
        if (handed) onClose();
      }}
      title={`Mật khẩu tạm của ${name}`}
      description="Chỉ hiện một lần."
      footer={
        <button type="button" className="btn btn-primary" disabled={!handed} onClick={onClose}>
          Xong
        </button>
      }
    >
      <div className="grid gap-4">
        <p>
          {name} đăng nhập bằng số {phone ?? "đã đăng ký"} và mật khẩu này, rồi đặt mật khẩu riêng ngay lần đầu. Mật khẩu này không xem lại được.
        </p>
        <div className="flex flex-wrap items-center gap-3">
          <code className="temp-password" aria-label="Mật khẩu tạm">
            {password}
          </code>
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => {
              void navigator.clipboard.writeText(password).then(() => {
                setCopied(true);
              });
            }}
          >
            {copied ? "Đã sao chép" : "Sao chép"}
          </button>
        </div>
        <label className="choice">
          <input
            type="checkbox"
            checked={handed}
            onChange={(e) => {
              setHanded(e.target.checked);
            }}
          />
          Tôi đã đưa mật khẩu cho nhân viên
        </label>
      </div>
    </BottomSheet>
  );
}

type Shown = { password: string; phone: string | null } | null;

/** A login asked for while adding the profile ("Thêm nhân viên"): created as the sheet opens. */
export interface PendingLogin {
  phone: string;
  role: LoginRole;
}

/**
 * "Tài khoản đăng nhập" of a profile (owner, ADR-0016): the profile and its login are one person
 * by construction — either "Tạo tài khoản đăng nhập" (phone + role) or the login itself (phone,
 * role, locked state) with "Đặt lại mật khẩu" and "Đổi vai trò". An owner's login is shown, never
 * changed here. Every action asks for the owner's password again when the server says so.
 */
export function LoginSection({ profile: p, pending = null }: { profile: StaffProfile; pending?: PendingLogin | null }) {
  const queryClient = useQueryClient();
  const login = p.details?.login;
  const [phone, setPhone] = useState(pending?.phone ?? "");
  const [role, setRole] = useState<LoginRole>(pending?.role ?? "technician");
  const [shown, setShown] = useState<Shown>(null);
  const [retry, setRetry] = useState<(() => void) | null>(null);
  const [confirmReset, setConfirmReset] = useState(false);
  const [newRole, setNewRole] = useState<LoginRole | "">("");
  const stepUp = (error: unknown, again: () => void) => {
    if (needsReauth(error)) setRetry(() => again);
  };
  const create = useMutation({
    mutationFn: () => createLogin(p.id, p.version, phone.trim(), role),
    onSuccess: async (r) => {
      setShown({ password: r.temporary_password, phone: r.profile.details?.login?.phone ?? phone.trim() });
      await invalidateStaff(queryClient);
    },
    onError: (e) => {
      stepUp(e, () => {
        create.mutate();
      });
    },
  });
  const reset = useMutation({
    mutationFn: () => resetLoginPassword(p.id),
    onSuccess: async (password) => {
      setConfirmReset(false);
      setShown({ password, phone: login?.phone ?? null });
      await invalidateStaff(queryClient);
    },
    onError: (e) => {
      stepUp(e, () => {
        reset.mutate();
      });
    },
  });
  const changeRole = useMutation({
    mutationFn: (r: LoginRole) => setLoginRole(p.id, r),
    onSuccess: async () => {
      setNewRole("");
      await invalidateStaff(queryClient);
    },
    onError: (e, r) => {
      stepUp(e, () => {
        changeRole.mutate(r);
      });
    },
  });
  const failure = [create.error, reset.error, changeRole.error].find((e) => e && !needsReauth(e));
  // "Tạo tài khoản đăng nhập ngay" from the add form: once, as soon as the new profile shows.
  const started = useRef(false);
  useEffect(() => {
    if (pending && !login && !started.current) {
      started.current = true;
      create.mutate();
    }
  }, [pending, login, create]);
  const manageable = login && (login.role === "manager" || login.role === "technician") && !login.locked;

  return (
    <section className="grid gap-3" aria-labelledby="roster-login">
      <h3 id="roster-login" className="panel-title">
        Tài khoản đăng nhập
      </h3>
      {!login && p.login_linked ? (
        <p className="text-sm text-text-muted">Đã gắn tài khoản đăng nhập.</p>
      ) : !login ? (
        p.status === "active" ? (
          <form
            className="grid gap-3"
            onSubmit={(e) => {
              e.preventDefault();
              if (!create.isPending) create.mutate();
            }}
          >
            <p className="text-sm text-text-muted">
              Tạo tài khoản để {p.full_name} tự xem ngày công, phiếu lương và (quản lý) ghi sổ. Hệ thống tạo một mật khẩu tạm, chỉ hiện một lần để bạn đưa cho nhân viên.
            </p>
            <div className="grid gap-3 sm:grid-cols-2">
              <label className="filter-field">
                Số điện thoại đăng nhập
                <input
                  inputMode="tel"
                  autoComplete="off"
                  required
                  value={phone}
                  aria-invalid={fieldMessage(create.error, "phone") ? true : undefined}
                  onChange={(e) => {
                    setPhone(e.target.value);
                  }}
                />
                {fieldMessage(create.error, "phone") && <span className="text-sm text-danger">Số điện thoại không hợp lệ (di động Việt Nam).</span>}
              </label>
              <label className="filter-field">
                Vai trò
                <select
                  value={role}
                  onChange={(e) => {
                    setRole(e.target.value as LoginRole);
                  }}
                >
                  {ROLES.map((r) => (
                    <option key={r} value={r}>
                      {LOGIN_ROLE_LABEL[r]}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            <div>
              <button type="submit" className="btn btn-primary" disabled={!phone.trim()} aria-busy={create.isPending || undefined}>
                Tạo tài khoản đăng nhập
              </button>
            </div>
          </form>
        ) : (
          <p className="text-sm text-text-muted">Chưa có tài khoản. Nhận lại làm để tạo tài khoản.</p>
        )
      ) : (
        <div className="grid gap-3">
          <dl className="roster-facts">
            <div>
              <dt>Số đăng nhập</dt>
              <dd>{login.phone ?? "—"}</dd>
            </div>
            <div>
              <dt>Vai trò</dt>
              <dd>{login.role === "owner" ? "Chủ spa" : login.role === "other" ? p.position : LOGIN_ROLE_LABEL[login.role]}</dd>
            </div>
            <div>
              <dt>Trạng thái</dt>
              <dd>{login.locked ? <span className="badge badge-danger">Đã khóa</span> : p.status === "left" ? "Chỉ xem công và lương cũ" : <span className="badge badge-ok">Đang dùng</span>}</dd>
            </div>
          </dl>
          {login.locked && <p className="text-sm text-text-muted">Đã khóa khi cho nghỉ việc. “Nhận lại làm” mở lại tài khoản.</p>}
          {manageable && (
            <div className="flex flex-wrap items-end gap-2">
              {confirmReset ? (
                <>
                  <span className="text-sm">Đặt mật khẩu tạm mới và đăng xuất {p.full_name} khỏi mọi thiết bị?</span>
                  <button type="button" className="btn btn-danger" aria-busy={reset.isPending || undefined} onClick={() => { if (!reset.isPending) reset.mutate(); }}>
                    Đặt lại
                  </button>
                  <button type="button" className="btn btn-secondary" onClick={() => { setConfirmReset(false); }}>
                    Thôi
                  </button>
                </>
              ) : (
                <button type="button" className="btn btn-secondary" onClick={() => { setConfirmReset(true); }}>
                  Đặt lại mật khẩu
                </button>
              )}
              {p.status === "active" && (
                <>
                  <label className="filter-field">
                    Đổi vai trò
                    <select
                      value={newRole || login.role}
                      onChange={(e) => {
                        setNewRole(e.target.value as LoginRole);
                      }}
                    >
                      {ROLES.map((r) => (
                        <option key={r} value={r}>
                          {LOGIN_ROLE_LABEL[r]}
                        </option>
                      ))}
                    </select>
                  </label>
                  <button
                    type="button"
                    className="btn btn-secondary"
                    disabled={!newRole || newRole === login.role}
                    aria-busy={changeRole.isPending || undefined}
                    onClick={() => {
                      if (newRole && !changeRole.isPending) changeRole.mutate(newRole);
                    }}
                  >
                    Lưu vai trò
                  </button>
                </>
              )}
            </div>
          )}
        </div>
      )}
      {failure && !fieldMessage(failure, "phone") && (
        <p role="alert" className="text-sm text-danger">
          {messageFor(failure)}
        </p>
      )}
      {retry && (
        <ReauthDialog
          onClose={() => {
            setRetry(null);
          }}
          onDone={() => {
            const again = retry;
            setRetry(null);
            again();
          }}
        />
      )}
      {shown && (
        <TemporaryPasswordDialog
          name={p.full_name}
          phone={shown.phone}
          password={shown.password}
          onClose={() => {
            setShown(null);
          }}
        />
      )}
    </section>
  );
}
