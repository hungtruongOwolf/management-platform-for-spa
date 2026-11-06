import { useQuery, useSuspenseQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { meQuery } from "../features/auth/session";
import { spaToday } from "../features/corrections/api";
import { dailyReportQuery } from "../features/reports/api";
import { StatCard, StatSkeleton, count, money } from "../features/reports/ui";
import { BrandLogo } from "./BrandLogo";
import { canSeeManagement } from "./features";
import { Icon } from "./Icon";
import { longDate } from "../lib/format";
import { navFor, navKey } from "./nav";

const HINTS: Record<string, string> = {
  "/quay": "Tìm khách, trừ buổi, thu tiền",
  "/khach-hang": "Tìm khách, hồ sơ, gói liệu trình, lịch sử",
  "/khach-lau-khong-quay-lai": "Khách vắng từ 7 ngày, cần gọi nhắc",
  "/goi-lieu-trinh": "Gói sắp hết hạn, hết buổi, đã hủy",
  "/bao-cao": "Thực thu, khách, lượt theo kỳ; sổ doanh thu",
  "/bang-gia": "Dịch vụ theo nhóm, giá Nữ/Nam",
  "/bang-gia/goi": "Số buổi, giá, quà tặng, hạn dùng của từng gói",
};

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
  const shortcuts = navFor(me)
    .flatMap((g) => g.items)
    // One shortcut per screen: tabs inside a screen (Lịch hẹn) are reached from that screen.
    .filter((i) => navKey(i) in HINTS);
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
          {shortcuts.map((s) => (
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
                  {HINTS[s.to]}
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
