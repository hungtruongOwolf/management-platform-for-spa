import { useQuery } from "@tanstack/react-query";
import { Icon } from "../../app/Icon";
import { accountsQuery, profilesQuery } from "./api";
import { managerLoginWithoutProfile, managerWithoutLogin } from "./roster";

/**
 * F19 (live test 2026-11-08), for the owner only (`staff.manage`: both lists are theirs to read):
 * the managers whose profile and login are not linked. Decision C knows a manager's own profile
 * only through that link; until it exists the manager cannot read their own month, and a "Quản lý"
 * profile is the owner's to record (the server refuses a manager, `staff.record_owner_only`).
 * Shown in Sổ hằng ngày and Nhân sự; `onOpenRoster` (the daily book) leads to where it is fixed.
 * Nothing renders while loading, on error or when every manager is linked.
 */
export function LinkWarnings({ onOpenRoster }: { onOpenRoster?: () => void }) {
  const profiles = useQuery(profilesQuery);
  const accounts = useQuery(accountsQuery);
  const profilesLeft = (profiles.data ?? []).filter(managerWithoutLogin);
  const loginsLeft = (accounts.data ?? []).filter(managerLoginWithoutProfile);
  if (profilesLeft.length === 0 && loginsLeft.length === 0) return null;
  return (
    <section role="status" aria-labelledby="link-warning-title" className="callout">
      <h2 id="link-warning-title" className="flex items-center gap-2 font-semibold">
        <Icon name="alert" className="size-4" />
        Chưa gắn tài khoản quản lý với hồ sơ
      </h2>
      <ul className="grid gap-1">
        {profilesLeft.map((p) => (
          <li key={p.id}>
            Hồ sơ <strong>{p.full_name}</strong> ({p.position}) chưa gắn tài khoản đăng nhập.
          </li>
        ))}
        {loginsLeft.map((a) => (
          <li key={a.user_id}>
            Tài khoản quản lý <strong>{a.full_name}</strong> chưa gắn với hồ sơ nhân viên nào.
          </li>
        ))}
      </ul>
      <p>
        Khi chưa gắn, ứng dụng không biết hồ sơ nào là của quản lý: quản lý không xem được công và lương của mình, và chỉ chủ spa chấm công, ghi khen/phàn nàn, thưởng/phạt, đánh giá cho hồ sơ “Quản lý”. Mở hồ sơ ở Nhân sự → “Tài khoản đăng nhập” để gắn.
      </p>
      {onOpenRoster && (
        <div>
          <button type="button" className="btn btn-secondary" onClick={onOpenRoster}>
            Mở Nhân sự
          </button>
        </div>
      )}
    </section>
  );
}
