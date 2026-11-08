import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Icon } from "../../app/Icon";
import { messageFor } from "../../lib/errors";
import {
  accountsQuery,
  attendanceQuery,
  evaluationsQuery,
  invalidateStaff,
  profilesQuery,
  setAccount,
  type StaffAccount,
  type StaffProfile,
} from "./api";
import { legacyLogin, withoutLogin } from "./roster";

/**
 * For the owner only (`staff.manage`: both lists are theirs to read). Logins are created from a
 * profile (ADR-0016), so a login with no profile exists only in older data (made with the CLI):
 * this notice lists them — a manager among them records nothing until linked
 * (`403 staff.link_required`) — each with a one-time "Gắn vào hồ sơ". It shows only when there is
 * such a login. With `nudge` (Hồ sơ nhân viên), the people still without a login are listed too:
 * they count as technicians and cannot read their own month.
 */
export function LinkWarnings({ onOpenRoster, nudge = false }: { onOpenRoster?: () => void; nudge?: boolean }) {
  const profiles = useQuery(profilesQuery);
  const accounts = useQuery(accountsQuery);
  const loginsLeft = (accounts.data ?? []).filter(legacyLogin);
  const free = (profiles.data ?? []).filter(withoutLogin);
  const profilesLeft: StaffProfile[] = nudge ? free : [];
  if (loginsLeft.length === 0 && profilesLeft.length === 0) return null;
  return (
    <>
      {loginsLeft.length > 0 && (
        <section role="status" aria-labelledby="link-warning-title" className="callout">
          <h2 id="link-warning-title" className="flex items-center gap-2 font-semibold">
            <Icon name="alert" className="size-4" />
            Tài khoản đăng nhập chưa gắn với hồ sơ
          </h2>
          <p>
            Các tài khoản này được tạo trước đây, chưa thuộc hồ sơ nhân viên nào: ứng dụng không biết đó là ai, nên quản lý chưa ghi sổ được và không ai xem được công và lương của mình. Gắn mỗi tài khoản vào đúng hồ sơ một lần.
          </p>
          {/* Rows, not list items: the daily book's cards are the page's list items. */}
          <div className="grid gap-2">
            {loginsLeft.map((a) => (
              <LegacyRow key={a.user_id} account={a} profiles={free} />
            ))}
          </div>
          {onOpenRoster && (
            <div>
              <button type="button" className="btn btn-secondary" onClick={onOpenRoster}>
                Mở Hồ sơ nhân viên
              </button>
            </div>
          )}
        </section>
      )}
      {profilesLeft.length > 0 && (
        <p role="status" className="note">
          Chưa có tài khoản đăng nhập: {profilesLeft.map((p) => p.full_name).join(", ")}. Mở hồ sơ → “Tạo tài khoản đăng nhập” để họ tự xem ngày công và phiếu lương (khi chưa có tài khoản, hồ sơ được tính như KTV).
        </p>
      )}
    </>
  );
}

/** One login with no profile: pick the profile it belongs to, link once (`PUT /staff/{id}/account`). */
function LegacyRow({ account: a, profiles }: { account: StaffAccount; profiles: StaffProfile[] }) {
  const queryClient = useQueryClient();
  const [target, setTarget] = useState("");
  const link = useMutation({
    mutationFn: () => {
      const p = profiles.find((x) => x.id === target);
      if (!p) throw new Error("no profile");
      return setAccount(p.id, p.version, a.user_id);
    },
    onSuccess: () => invalidateStaff(queryClient),
  });
  const id = `legacy-${a.user_id}`;
  return (
    <div className="flex flex-wrap items-end gap-2">
      <span className="min-w-0 flex-1">
        <strong>{a.full_name}</strong> ({a.roles.join(", ")})
      </span>
      <label className="filter-field" htmlFor={id}>
        Hồ sơ của {a.full_name}
        <select
          id={id}
          value={target}
          onChange={(e) => {
            setTarget(e.target.value);
          }}
        >
          <option value="">Chọn hồ sơ…</option>
          {profiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.full_name}
            </option>
          ))}
        </select>
      </label>
      <button
        type="button"
        className="btn btn-secondary"
        disabled={!target}
        aria-busy={link.isPending || undefined}
        onClick={() => {
          if (!link.isPending) link.mutate();
        }}
      >
        Gắn vào hồ sơ
      </button>
      {link.isError && (
        <span role="alert" className="w-full text-sm text-danger">
          {messageFor(link.error)}
        </span>
      )}
    </div>
  );
}

/**
 * A manager whose login is linked to no profile records nothing (`staff.link_required`, EM
 * 2026-11-08): the sheet they are looking at says so (`link_required`), and this tells them,
 * plainly, what to do. Reads the query the tab already holds (attendance, or the evaluations).
 */
export function LinkRequiredNotice({ month, fromAttendance }: { month: string; fromAttendance: boolean }) {
  const attendance = useQuery({ ...attendanceQuery(month), enabled: fromAttendance });
  const evaluations = useQuery({ ...evaluationsQuery(month), enabled: !fromAttendance });
  const required = fromAttendance ? attendance.data?.link_required : evaluations.data?.link_required;
  if (!required) return null;
  return (
    <p role="status" className="callout flex items-start gap-2">
      <Icon name="alert" className="mt-0.5 size-4 shrink-0" />
      <span>
        Tài khoản của bạn chưa gắn với hồ sơ nhân viên, nên bạn chỉ xem được. Nhờ chủ spa gắn tài khoản vào hồ sơ của bạn ở
        Hồ sơ nhân viên để chấm công, ghi khen/phàn nàn, thưởng/phạt và đánh giá.
      </span>
    </p>
  );
}
