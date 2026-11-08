import { useQuery } from "@tanstack/react-query";
import { Icon } from "../../app/Icon";
import { accountsQuery, attendanceQuery, evaluationsQuery, profilesQuery, type StaffProfile } from "./api";
import { managerLoginWithoutProfile, withoutLogin } from "./roster";

/**
 * For the owner only (`staff.manage`: both lists are theirs to read). Who someone is comes from
 * their account role (EM 2026-11-08), so the warnings read roles, never titles:
 * - a manager login (records for others, does not manage staff) linked to no profile records
 *   nothing until the owner links it (`403 staff.link_required`): a warning, in Sổ hằng ngày and
 *   Nhân sự; `onOpenRoster` (the daily book) leads to where it is fixed;
 * - with `nudge` (Nhân sự), the people still without a login: they count as technicians and
 *   cannot read their own month, so the owner is nudged to give everyone an account.
 * Nothing renders while loading, on error or when there is nothing to say.
 */
export function LinkWarnings({ onOpenRoster, nudge = false }: { onOpenRoster?: () => void; nudge?: boolean }) {
  const profiles = useQuery({ ...profilesQuery, enabled: nudge });
  const accounts = useQuery(accountsQuery);
  const loginsLeft = (accounts.data ?? []).filter(managerLoginWithoutProfile);
  const profilesLeft: StaffProfile[] = nudge ? (profiles.data ?? []).filter(withoutLogin) : [];
  if (loginsLeft.length === 0 && profilesLeft.length === 0) return null;
  return (
    <>
      {loginsLeft.length > 0 && (
        <section role="status" aria-labelledby="link-warning-title" className="callout">
          <h2 id="link-warning-title" className="flex items-center gap-2 font-semibold">
            <Icon name="alert" className="size-4" />
            Tài khoản quản lý chưa gắn với hồ sơ
          </h2>
          <ul className="grid gap-1">
            {loginsLeft.map((a) => (
              <li key={a.user_id}>
                <strong>{a.full_name}</strong> ({a.roles.join(", ")}) chưa gắn với hồ sơ nhân viên nào.
              </li>
            ))}
          </ul>
          <p>
            Khi chưa gắn, ứng dụng không biết hồ sơ nào là của quản lý đó, nên quản lý chưa chấm công, ghi khen/phàn nàn, thưởng/phạt hay đánh giá được, và chưa xem được công và lương của mình. Mở hồ sơ của họ ở Hồ sơ nhân viên → “Tài khoản đăng nhập” để gắn.
          </p>
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
          Chưa có tài khoản đăng nhập: {profilesLeft.map((p) => p.full_name).join(", ")}. Gắn tài khoản để họ tự xem ngày công và phiếu lương (khi chưa có tài khoản, hồ sơ được tính như KTV).
        </p>
      )}
    </>
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
