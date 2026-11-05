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
        mgmt && { to: "/khach-lau-khong-quay-lai", label: "Lâu không quay lại", icon: "user-clock" },
      ],
    ],
    ["Gói liệu trình", [mgmt && { to: "/goi-lieu-trinh", label: "Theo dõi gói", icon: "package" }]],
    ["Báo cáo", [mgmt && { to: "/bao-cao", label: "Kinh doanh", icon: "chart" }]],
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

/**
 * Short names of the screens, for a back control whose destination has no PageHeader title (the
 * front desk, home). Paths not listed fall back to the nearest listed parent.
 */
const SCREEN_LABELS: Record<string, string> = {
  "/": "Tổng quan",
  "/quay": "Quầy",
  "/khach-hang": "Khách hàng",
  "/khach-lau-khong-quay-lai": "Lâu không quay lại",
  "/goi-lieu-trinh": "Theo dõi gói",
  "/bao-cao": "Báo cáo",
  "/bang-gia": "Dịch vụ",
  "/bang-gia/goi": "Gói bán",
  "/cai-dat": "Cài đặt",
};

export function screenLabel(pathname: string): string {
  for (let p = pathname; p.length > 0; p = p.slice(0, Math.max(0, p.lastIndexOf("/")))) {
    const label = SCREEN_LABELS[p];
    if (label) return label;
  }
  return "Tổng quan";
}

/** Phone tab bar: the four most used screens; everything else sits in the "Thêm" drawer. */
export function tabsFor(groups: NavGroup[]): NavItem[] {
  const all = groups.flatMap((g) => g.items);
  const wanted = ["/", "/quay", "/khach-hang", "/bao-cao"];
  return wanted.flatMap((to) => all.filter((i) => i.to === to));
}
