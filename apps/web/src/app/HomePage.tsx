import { useQuery, useSuspenseQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { meQuery } from "../features/auth/session";
import { spaToday } from "../features/corrections/api";
import { dailyReportQuery } from "../features/reports/api";
import { CountUp, StatCard, StatSkeleton, money } from "../features/reports/ui";
import { BrandLogo } from "./BrandLogo";
import { canSeeManagement } from "./features";
import { Icon } from "./Icon";
import { navFor } from "./nav";

const longDate = new Intl.DateTimeFormat("vi-VN", {
  weekday: "long",
  day: "numeric",
  month: "numeric",
  year: "numeric",
  timeZone: "Asia/Ho_Chi_Minh",
});

const HINTS: Record<string, string> = {
  "/quay": "Tìm khách, trừ buổi, thu tiền",
  "/khach-hang": "Liệu trình, lịch sử, lưu ý sức khỏe",
  "/khach-lau-khong-quay-lai": "Khách vắng từ 7 ngày, cần gọi nhắc",
  "/goi-lieu-trinh": "Gói sắp hết hạn, hết buổi",
  "/bao-cao": "Khách, lượt, thực thu trong ngày",
  "/bang-gia": "Menu, giá Nữ/Nam",
  "/bang-gia/goi": "Các gói liệu trình đang bán",
};

function Today() {
  const daily = useQuery(dailyReportQuery(spaToday()));
  return (
    <section aria-labelledby="today-title" className="grid content-start gap-3">
      <div className="flex items-center justify-between gap-2">
        <h2 id="today-title" className="text-lg font-semibold">
          Hôm nay
        </h2>
        <Link to="/bao-cao" className="inline-flex min-h-11 items-center gap-1 text-sm font-semibold text-primary hover:underline">
          Xem báo cáo <Icon name="arrow-right" className="size-4" />
        </Link>
      </div>
      <div className="stat-grid">
        {daily.data ? (
          <>
            {daily.data.revenue && (
              <StatCard label="Thực thu" icon="wallet" accent>
                <CountUp value={daily.data.revenue.net.total} format={money} />
              </StatCard>
            )}
            <StatCard label="Khách" icon="users">
              <CountUp value={daily.data.counts.customers} />
            </StatCard>
            <StatCard label="Buổi liệu trình đã trừ" icon="package">
              <CountUp value={daily.data.counts.sessions_used} />
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
    .filter((i) => i.to in HINTS);
  return (
    <div className="grid content-start gap-6">
      {/* The whole logo carries the tagline, so the greeting does not repeat it (EM round 3). */}
      <section className="welcome chrome ornament [--ornament:var(--chrome-accent)]">
        <BrandLogo className="welcome-logo" />
        <div className="min-w-0">
          <p className="text-sm text-chrome-text-muted first-letter:uppercase">{longDate.format(new Date())}</p>
          <h1 className="mt-0.5 font-brand text-2xl text-chrome-text sm:text-[28px] sm:leading-9">Xin chào, {me?.user.full_name}</h1>
          <p className="mt-1 text-sm text-chrome-text-muted">{me?.active_tenant?.name ?? "Chưa chọn spa"}</p>
        </div>
      </section>

      {canSeeManagement(me) && <Today />}

      <section aria-labelledby="shortcuts-title" className="grid content-start gap-3">
        <h2 id="shortcuts-title" className="text-lg font-semibold">
          Lối tắt
        </h2>
        <nav aria-label="Lối tắt" className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {shortcuts.map((s) => (
            <Link key={s.to} to={s.to} className="card card-link group flex min-h-[72px] items-center gap-3 px-4 py-3">
              <span aria-hidden="true" className="grid size-10 shrink-0 place-items-center rounded-[10px] bg-primary-soft text-primary">
                <Icon name={s.icon} className="size-5" />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block font-semibold text-primary">{s.label}</span>
                <span className="block text-sm text-text-muted">{HINTS[s.to]}</span>
              </span>
              <Icon name="chevron-right" className="size-5 text-text-muted transition-transform group-hover:translate-x-0.5" />
            </Link>
          ))}
        </nav>
      </section>
    </div>
  );
}
