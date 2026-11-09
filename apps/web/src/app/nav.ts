import { can, type Me } from "../features/auth/session";
import { mayOpen } from "./access";
import type { IconName } from "./Icon";

export interface NavItem {
  to: string;
  /** Nav item = page title (glossary § Tên màn hình). */
  label: string;
  /** One-line name for the phone tab bar (≤ 10 characters). */
  short?: string;
  icon: IconName;
  /** Only this exact path marks the item active (e.g. "/" would otherwise match everything). */
  exact?: boolean;
  /**
   * A tab inside a screen (e.g. `/quay?view=lich`). With `search`, the item is active only when
   * the URL carries exactly these values; `undefined` means "the key is absent" (the default tab).
   */
  search?: Record<string, string | undefined>;
}

/** Stable id of an item (path + its search), for keys and the command palette. */
export function navKey(item: NavItem): string {
  const params = Object.entries(item.search ?? {}).filter(
    (e): e is [string, string] => e[1] !== undefined,
  );
  return params.length
    ? `${item.to}?${new URLSearchParams(params).toString()}`
    : item.to;
}

const APPOINTMENTS = "/quay?view=lich";

export interface NavGroup {
  label: string;
  items: NavItem[];
}

/**
 * The app's navigation, filtered by what the signed-in user may do. UX only: every route's API
 * answers 403 on its own. One model feeds the sidebar, the phone drawer and the command palette.
 */
export function navFor(me: Me | null): NavGroup[] {
  const mgmt = mayOpen.management(me);
  const desk = mayOpen.frontDesk(me);
  const catalog = mayOpen.catalog(me);
  const groups: [string, (NavItem | false)[]][] = [
    [
      "Quầy",
      [
        {
          to: "/",
          label: "Tổng quan",
          short: "Trang chủ",
          icon: "home",
          exact: true,
        },
        desk && {
          to: "/quay",
          label: "Ghi lượt khách",
          short: "Quầy",
          icon: "desk",
          // Not active on the appointments tab of the same screen.
          ...(can(me, "appointment.manage")
            ? { search: { view: undefined } }
            : {}),
        },
        desk &&
          can(me, "appointment.manage") && {
            to: "/quay",
            search: { view: "lich" },
            label: "Lịch hẹn",
            short: "Lịch hẹn",
            icon: "calendar",
          },
      ],
    ],
    [
      "Khách hàng",
      [
        mayOpen.customers(me) && {
          to: "/khach-hang",
          label: "Danh sách khách hàng",
          short: "Khách",
          icon: "users",
        },
        mgmt && {
          to: "/khach-lau-khong-quay-lai",
          label: "Khách lâu chưa quay lại",
          icon: "user-clock",
        },
      ],
    ],
    [
      "Gói liệu trình",
      [
        mgmt && {
          to: "/goi-lieu-trinh",
          label: "Gói liệu trình của khách",
          icon: "package",
        },
      ],
    ],
    [
      "Báo cáo",
      [
        mayOpen.reports(me) && {
          to: "/bao-cao",
          label: "Báo cáo kinh doanh",
          short: "Báo cáo",
          icon: "chart",
        },
        // Reading back what was written is a review, like the reports (spec management-views §10).
        mayOpen.notes(me) && {
          to: "/nhat-ky-ghi-chu",
          label: "Nhật ký ghi chú",
          icon: "history",
        },
      ],
    ],
    ["Nhân viên", [staffItem(me), myPayItem(me), myWorkItem(me)]],
    [
      "Danh mục",
      [
        catalog && {
          to: "/bang-gia",
          label: "Bảng giá dịch vụ",
          icon: "menu-book",
          exact: true,
        },
        catalog && {
          to: "/bang-gia/goi",
          label: "Mẫu gói liệu trình",
          icon: "layers",
        },
      ],
    ],
    [
      "Cài đặt",
      [{ to: "/cai-dat", label: "Giao diện & tài khoản", icon: "settings" }],
    ],
  ];
  return groups
    .map(([label, items]) => ({
      label,
      items: items.filter((i): i is NavItem => i !== false),
    }))
    .filter((g) => g.items.length > 0);
}

/**
 * Staff who keep the books without managing staff: a manager. Their own profile is not in the
 * books (EM 2026-11-08), so they read their own month in "Công và lương của tôi", like a
 * technician. The owner records and sees everyone, themselves included.
 */
function keepsBooksForOthers(me: Me | null): boolean {
  return mayOpen.staffBooks(me) && !can(me, "staff.manage");
}

/** Staff management for those who keep the books; "my month" for everyone else linked to a profile. */
function staffItem(me: Me | null): NavItem | false {
  if (mayOpen.staffBooks(me))
    return {
      to: "/nhan-vien",
      label: "Quản lý nhân sự",
      icon: "id-card",
      // Not active on /nhan-vien/cua-toi, the manager's own month.
      ...(keepsBooksForOthers(me) && mayOpen.myPay(me) ? { exact: true } : {}),
    };
  return mayOpen.myPay(me) && { to: "/nhan-vien", label: "Công và lương của tôi", short: "Công lương", icon: "id-card" };
}

/** A manager's own month (EM 2026-11-08): attendance, pay, evaluation and its history. */
function myPayItem(me: Me | null): NavItem | false {
  return (
    keepsBooksForOthers(me) &&
    mayOpen.myPay(me) && { to: "/nhan-vien/cua-toi", label: "Công và lương của tôi", short: "Công lương", icon: "wallet" }
  );
}

/**
 * "Khách của tôi" (round 5): the customers and bookings of the signed-in staff member. A
 * technician's only view of visits; a manager also gets it (EM 2026-11-08: their own things the
 * way a technician sees them; useful when they serve customers too). The owner sees everything at
 * the front desk.
 */
function myWorkItem(me: Me | null): NavItem | false {
  return (
    mayOpen.myWork(me) &&
    (!can(me, "visit.read") || keepsBooksForOthers(me)) && {
      to: "/khach-cua-toi",
      label: "Khách của tôi",
      short: "Khách",
      icon: "history",
    }
  );
}

/**
 * Short names of the screens, for a back control whose destination has no PageHeader title (the
 * front desk, home). Paths not listed fall back to the nearest listed parent.
 */
const SCREEN_LABELS: Record<string, string> = {
  "/": "Tổng quan",
  "/quay": "Quầy",
  "/khach-hang": "Khách hàng",
  "/khach-lau-khong-quay-lai": "Khách lâu chưa quay lại",
  "/goi-lieu-trinh": "Gói liệu trình của khách",
  "/bao-cao": "Báo cáo kinh doanh",
  "/bang-gia": "Bảng giá dịch vụ",
  "/bang-gia/goi": "Mẫu gói liệu trình",
  "/nhan-vien": "Quản lý nhân sự",
  "/nhan-vien/cua-toi": "Công và lương của tôi",
  "/khach-cua-toi": "Khách của tôi",
  "/cai-dat": "Cài đặt",
};

export function screenLabel(pathname: string): string {
  for (
    let p = pathname;
    p.length > 0;
    p = p.slice(0, Math.max(0, p.lastIndexOf("/")))
  ) {
    const label = SCREEN_LABELS[p];
    if (label) return label;
  }
  return "Tổng quan";
}

/**
 * Phone tab bar: the four most used screens; everything else sits in the "Thêm" drawer. With
 * appointments, "Lịch hẹn" takes the home tab's place (EM decision, round 4); home stays in "Thêm".
 */
export function tabsFor(groups: NavGroup[]): NavItem[] {
  const all = groups.flatMap((g) => g.items);
  const first = all.some((i) => navKey(i) === APPOINTMENTS)
    ? APPOINTMENTS
    : "/";
  // "/nhan-vien" and "/khach-cua-toi" only fill free slots (technicians: home, their month,
  // their customers).
  const wanted = [
    first,
    "/quay",
    "/khach-hang",
    "/bao-cao",
    "/nhan-vien",
    "/khach-cua-toi",
  ];
  return wanted.flatMap((key) => all.filter((i) => navKey(i) === key)).slice(0, 4);
}
