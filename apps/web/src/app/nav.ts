import { can, type Me } from "../features/auth/session";
import { canSeeManagement } from "./features";
import type { IconName } from "./Icon";

export interface NavItem {
  to: string;
  label: string;
  icon: IconName;
  /** Only this exact path marks the item active (e.g. "/" would otherwise match everything). */
  exact?: boolean;
}

export interface NavGroup {
  label: string;
  items: NavItem[];
}

/**
 * The app's navigation, filtered by what the signed-in user may do. UX only: every route's API
 * answers 403 on its own. One model feeds the sidebar, the phone drawer and the command palette.
 */
export function navFor(me: Me | null): NavGroup[] {
  const mgmt = canSeeManagement(me);
  const groups: [string, (NavItem | false)[]][] = [
    [
      "Quầy",
      [
        { to: "/", label: "Tổng quan", icon: "home", exact: true },
        can(me, "visit.record") && { to: "/quay", label: "Ghi lượt khách", icon: "desk" },
      ],
    ],
    [
      "Khách hàng",
      [
        can(me, "customer.read") && { to: "/khach-hang", label: "Hồ sơ khách", icon: "users" },
        mgmt && { to: "/nhac-khach", label: "Lâu chưa quay lại", icon: "user-clock" },
      ],
    ],
    ["Gói liệu trình", [mgmt && { to: "/goi-lieu-trinh", label: "Theo dõi gói", icon: "package" }]],
    ["Báo cáo", [mgmt && { to: "/bao-cao", label: "Hôm nay", icon: "chart" }]],
    [
      "Danh mục",
      [
        can(me, "catalog.read") && { to: "/bang-gia", label: "Dịch vụ", icon: "menu-book", exact: true },
        can(me, "catalog.read") && { to: "/bang-gia/goi", label: "Gói bán", icon: "layers" },
      ],
    ],
    ["Cài đặt", [{ to: "/cai-dat", label: "Giao diện & tài khoản", icon: "settings" }]],
  ];
  return groups
    .map(([label, items]) => ({ label, items: items.filter((i): i is NavItem => i !== false) }))
    .filter((g) => g.items.length > 0);
}

/** Phone tab bar: the four most used screens; everything else sits in the "Thêm" drawer. */
export function tabsFor(groups: NavGroup[]): NavItem[] {
  const all = groups.flatMap((g) => g.items);
  const wanted = ["/", "/quay", "/khach-hang", "/bao-cao"];
  return wanted.flatMap((to) => all.filter((i) => i.to === to));
}
