import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Icon } from "../../app/Icon";
import { ApiError } from "../../lib/api";
import { messageFor } from "../../lib/errors";
import { dismissResetRequest, invalidateStaff, resetLoginPassword, resetRequestsQuery, type PasswordResetRequest } from "./api";
import { ReauthDialog, TemporaryPasswordDialog } from "./LoginSection";
import "./staff.css";

const time = new Intl.DateTimeFormat("vi-VN", { hour: "2-digit", minute: "2-digit", day: "2-digit", month: "2-digit", hour12: false, timeZone: "Asia/Ho_Chi_Minh" });

/**
 * "Quên mật khẩu?" requests waiting for the owner (option A, ADR-0016), on Tổng quan and in
 * Quản lý nhân sự → Hồ sơ nhân viên: "Nguyễn Thị Lan xin đặt lại mật khẩu", each with "Đặt lại"
 * (a temporary password shown once, the person's sessions end) and "Bỏ qua". Renders nothing
 * when there is no request. Owner only (`staff.manage`); the server answers 403 to anyone else.
 */
export function ResetRequestsNotice() {
  const requests = useQuery(resetRequestsQuery);
  const list = requests.data ?? [];
  if (list.length === 0) return null;
  return (
    <section role="status" aria-labelledby="reset-requests-title" className="callout">
      <h2 id="reset-requests-title" className="flex items-center gap-2 font-semibold">
        <Icon name="alert" className="size-4" />
        Yêu cầu đặt lại mật khẩu
      </h2>
      <ul className="grid gap-2">
        {list.map((r) => (
          <RequestRow key={r.id} request={r} />
        ))}
      </ul>
    </section>
  );
}

function RequestRow({ request: r }: { request: PasswordResetRequest }) {
  const queryClient = useQueryClient();
  const [shown, setShown] = useState<string | null>(null);
  const [reauth, setReauth] = useState(false);
  const done = async () => {
    await queryClient.invalidateQueries({ queryKey: resetRequestsQuery.queryKey });
    await invalidateStaff(queryClient);
  };
  const reset = useMutation({
    mutationFn: () => resetLoginPassword(r.staff_id ?? ""),
    onSuccess: (password) => {
      setShown(password);
    },
    onError: (e) => {
      if (e instanceof ApiError && e.code === "auth.reauth_required") setReauth(true);
    },
  });
  const dismiss = useMutation({ mutationFn: () => dismissResetRequest(r.id), onSuccess: done });
  const error = [reset.error, dismiss.error].find((e) => e && !(e instanceof ApiError && e.code === "auth.reauth_required"));
  return (
    <li className="flex flex-wrap items-center gap-2">
      <span className="min-w-0 flex-1">
        <strong>{r.full_name}</strong> xin đặt lại mật khẩu · {time.format(new Date(r.requested_at))}
      </span>
      {r.staff_id ? (
        <button type="button" className="btn btn-secondary" aria-busy={reset.isPending || undefined} onClick={() => { if (!reset.isPending) reset.mutate(); }}>
          Đặt lại
        </button>
      ) : (
        <span className="text-sm">Gắn tài khoản vào hồ sơ trước</span>
      )}
      <button type="button" className="btn btn-ghost" aria-busy={dismiss.isPending || undefined} onClick={() => { if (!dismiss.isPending) dismiss.mutate(); }}>
        Bỏ qua
      </button>
      {error && (
        <span role="alert" className="w-full text-sm text-danger">
          {messageFor(error)}
        </span>
      )}
      {reauth && (
        <ReauthDialog
          onClose={() => {
            setReauth(false);
          }}
          onDone={() => {
            setReauth(false);
            reset.mutate();
          }}
        />
      )}
      {shown && (
        <TemporaryPasswordDialog
          name={r.full_name}
          phone={null}
          password={shown}
          onClose={() => {
            setShown(null);
            void done();
          }}
        />
      )}
    </li>
  );
}
