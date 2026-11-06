import { useQuery, useSuspenseQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { can, meQuery, type Me } from "../features/auth/session";
import { spaToday } from "../features/corrections/api";
import { dailyReportQuery } from "../features/reports/api";
import { StatCard, StatSkeleton, count, money } from "../features/reports/ui";
import { BrandLogo } from "./BrandLogo";
import { canSeeManagement } from "./features";
import { Icon } from "./Icon";
import { longDate } from "../lib/format";
import { navFor, navKey, type NavItem } from "./nav";

/** One line under each shortcut: what the screen is for. Screens without a hint get no shortcut. */
const HINTS: Record<string, string> = {
  "/quay": "Tìm khách, trừ buổi, thu tiền",
  "/khach-hang": "Tìm khách, hồ sơ, gói liệu trình, lịch sử",
  "/khach-lau-khong-quay-lai": "Khách vắng từ 7 ngày, cần gọi nhắc",
  "/goi-lieu-trinh": "Gói sắp hết hạn, hết buổi, đã hủy",
  "/bao-cao": "Thực thu, khách, lượt theo kỳ; sổ doanh thu",
  "/khach-cua-toi": "Khách bạn đã làm, lịch hẹn gán cho bạn",
  "/bang-gia": "Dịch vụ theo nhóm, giá Nữ/Nam",
  "/bang-gia/goi": "Số buổi, giá, quà tặng, hạn dùng của từng gói",
  "/cai-dat": "Giao diện sáng/tối, tài khoản đang đăng nhập",
};

/**
 * "/nhan-vien" opens different tabs per permission (StaffPage), so its hint lists only those;
 * staff who only see their own month get "Công và lương của tôi".
 */
function staffHint(me: Me | null): string {
  const parts = [
    can(me, "attendance.record") && "chấm công",
    can(me, "evaluation.record") && "đánh giá",
    can(me, "payroll.read") && "bảng lương",
    can(me, "staff.manage") && "hồ sơ nhân viên",
  ].filter((p): p is string => p !== false);
  const text = parts.length ? parts.join(", ") : "ngày công, đánh giá, phiếu lương của bạn";
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** The hint of a nav item, or undefined when the item is not a screen of its own (Lịch hẹn is a tab of /quay). */
function hintFor(item: NavItem, me: Me | null): string | undefined {
  if (navKey(item) === "/nhan-vien") return staffHint(me);
  return HINTS[navKey(item)];
}

function Today() {
  const daily = useQuery(dailyReportQuery(spaToday()));
  return (
    <section aria-labelledby="today-title" className="grid content-start gap-3">
      <div className="flex items-center justify-between gap-2">
        <h2 id="today-title" className="section-heading">
          Hôm nay
        </h2>
        <Link
          to="/bao-cao"
          className="link inline-flex min-h-11 items-center gap-1 text-sm"
        >
          Xem báo cáo <Icon name="arrow-right" className="size-4" />
        </Link>
      </div>
      <div className="stat-grid">
        {daily.data ? (
          <>
            {daily.data.revenue && (
              <StatCard label="Thực thu" icon="wallet" accent>
                {money(daily.data.revenue.net.total)}
              </StatCard>
            )}
            <StatCard label="Khách" icon="users">
              {count(daily.data.counts.customers)}
            </StatCard>
            <StatCard label="Buổi liệu trình đã trừ" icon="package">
              {count(daily.data.counts.sessions_used)}
            </StatCard>
          </>
        ) : (
          <StatSkeleton count={3} />
        )}
      </div>
    </section>
  );
}

/** "/" — greeting, today's numbers (where the management views exist) and shortcuts by permission. */
export function HomePage() {
  const { data: me } = useSuspenseQuery(meQuery);
  // One shortcut per screen: tabs inside a screen (Lịch hẹn) are reached from that screen, and
  // home is this page.
  const shortcuts = navFor(me)
    .flatMap((g) => g.items)
    .flatMap((item) => {
      const hint = hintFor(item, me);
      return hint ? [{ item, hint }] : [];
    });
  return (
    <div className="grid content-start gap-6">
      {/* The whole logo carries the tagline, so the greeting does not repeat it (EM round 3). */}
      <section className="welcome chrome">
        <BrandLogo className="welcome-logo" />
        <div className="min-w-0">
          <p className="text-sm text-chrome-text-muted tabular">
            {longDate(new Date())}
          </p>
          <h1 className="mt-0.5 font-brand text-2xl text-chrome-text sm:text-[28px] sm:leading-9">
            Xin chào, {me?.user.full_name}
          </h1>
          <p className="mt-1 text-sm text-chrome-text-muted">
            {me?.active_tenant?.name ?? "Chưa chọn spa"}
          </p>
        </div>
      </section>

      {canSeeManagement(me) && <Today />}

      <section
        aria-labelledby="shortcuts-title"
        className="grid content-start gap-3"
      >
        <h2 id="shortcuts-title" className="section-heading">
          Lối tắt
        </h2>
        <nav
          aria-label="Lối tắt"
          className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3"
        >
          {shortcuts.map(({ item: s, hint }) => (
            <Link
              key={s.to}
              to={s.to}
              className="card card-link shortcut"
            >
              <span
                aria-hidden="true"
                className="shortcut-icon"
              >
                <Icon name={s.icon} className="size-5" />
              </span>
              <span className="min-w-0 flex-1">
                <span className="shortcut-title">
                  {s.label}
                </span>
                <span className="block text-sm text-text-muted">
                  {hint}
                </span>
              </span>
              <Icon
                name="chevron-right"
                className="shortcut-chevron size-5"
              />
            </Link>
          ))}
        </nav>
      </section>
    </div>
  );
}
