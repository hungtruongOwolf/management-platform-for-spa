import { useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import {
  Link,
  Outlet,
  useNavigate,
  useRouterState,
} from "@tanstack/react-router";
import { Suspense, lazy, useEffect, useRef, useState, type ReactNode } from "react";
import { useBackdropClose, useModalDialog } from "../components/useModalDialog";
import { can, logout, meQuery } from "../features/auth/session";
import { longDate } from "../lib/format";
import { BrandLogo } from "./BrandLogo";
import { Icon } from "./Icon";
import { InShell } from "./inShell";
import { navFor, navKey, tabsFor, type NavGroup, type NavItem } from "./nav";

/** A tab inside a screen links with its search and is active only on exactly that search. */
function linkSearch(item: NavItem) {
  return item.search ? { search: item.search as never } : {};
}
function activeFor(item: NavItem) {
  return {
    exact: item.exact ?? false,
    includeSearch: item.search !== undefined,
    explicitUndefined: true,
  };
}
import { readSidebarCollapsed, saveSidebarCollapsed } from "./prefs";
import { resetTrail } from "./trail";
import { UserMenu } from "./UserMenu";
import { SkipLink } from "../components/SkipLink";

// Opens on Ctrl/⌘+K or the search button only: its own chunk, off the startup path.
const CommandPalette = lazy(() => import("./CommandPalette"));

function NavLinks({
  groups,
  onNavigate,
}: {
  groups: NavGroup[];
  onNavigate?: () => void;
}) {
  return (
    <>
      {groups.map((g) => (
        <div
          key={g.label}
          role="group"
          aria-labelledby={`nav-${g.label}`}
          className="nav-group"
        >
          <p id={`nav-${g.label}`} className="nav-group-label">
            {g.label}
          </p>
          {g.items.map((item) => (
            <Link
              key={navKey(item)}
              to={item.to}
              {...linkSearch(item)}
              activeOptions={activeFor(item)}
              className="nav-item"
              title={item.label}
              {...(onNavigate ? { onClick: onNavigate } : {})}
            >
              <Icon name={item.icon} />
              <span className="nav-label">{item.label}</span>
            </Link>
          ))}
        </div>
      ))}
    </>
  );
}

/**
 * Today's date in the top bar as a chip (EM round 2: "thứ ngày … có cái link hay component cho nó
 * rõ lên"): a calendar icon and the date; for whoever books appointments it opens today's calendar.
 */
function TodayChip({ canOpenCalendar }: { canOpenCalendar: boolean }) {
  const today = longDate(new Date());
  const face = (
    <>
      <Icon name="calendar" className="size-4" />
      <span>{today}</span>
    </>
  );
  return canOpenCalendar ? (
    <Link
      to="/quay"
      search={{ view: "lich" } as never}
      className="date-chip is-link tabular"
      aria-label={`${today}, mở lịch hẹn hôm nay`}
    >
      {face}
    </Link>
  ) : (
    <p className="date-chip tabular">{face}</p>
  );
}

/** `<dialog>` driven by React state: the browser gives focus trapping, Esc and the backdrop. */
export function Modal({
  open,
  onClose,
  className,
  label,
  children,
}: {
  open: boolean;
  onClose: () => void;
  className: string;
  label: string;
  children: ReactNode;
}) {
  const ref = useModalDialog(open);
  const backdrop = useBackdropClose(onClose);
  return (
    <dialog
      ref={ref}
      aria-label={label}
      className={className}
      onClose={onClose}
      {...backdrop}
    >
      {open && children}
    </dialog>
  );
}

/**
 * Signed-in frame. Desktop/tablet: a maroon sidebar (the whole logo, grouped nav, collapsible on
 * ≥ 1024px, an icon rail below that) and a top bar (date, search / command palette, account menu).
 * Phones: a top bar and a bottom tab bar with a "Thêm" drawer. Nav is filtered by permission (UX
 * only — the API decides).
 */
export function AppShell() {
  const { data: me } = useSuspenseQuery(meQuery);
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const groups = navFor(me);
  const canOpenCalendar = can(me, "visit.record") && can(me, "appointment.manage");
  const tabs = tabsFor(groups);
  const [collapsed, setCollapsed] = useState(readSidebarCollapsed);
  const [palette, setPalette] = useState(false);
  const [drawer, setDrawer] = useState(false);
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const href = useRouterState({ select: (s) => s.location.href });

  // Screens slide in after a navigation, never on the first paint (that would delay LCP).
  const [firstPath] = useState(pathname);
  const [navigated, setNavigated] = useState(false);
  if (!navigated && pathname !== firstPath) setNavigated(true);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette(true);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
    };
  }, []);

  // The session ended while the app was open (a 401 refetched `/me`, lib/queryClient.ts): forget
  // what was cached for it and sign in again, back to this screen. Unsaved carts stay in
  // sessionStorage. Sign-out and "Khóa nhanh" navigate on their own.
  // A sign-out or quick lock in progress: it chooses where to go, so the effect below stays out
  // of the way (otherwise it would race it and nest `next=/dang-nhap?next=…`).
  const leaving = useRef(false);
  useEffect(() => {
    if (me || leaving.current) return;
    queryClient.removeQueries({ predicate: (q) => q.queryKey[0] !== "me" });
    void navigate({ to: "/dang-nhap", search: { next: href } });
    // Only a change of `me` matters, not the URL it was on.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [me]);

  // "Khóa nhanh": leave the counter safely; signing in again comes back to this screen.
  const lock = async () => {
    leaving.current = true;
    const back = href;
    await logout(queryClient);
    await navigate({ to: "/dang-nhap", search: { next: back } });
  };
  // "Đăng xuất": a clean end of the shift, the next person starts from home with no trail.
  const signOut = async () => {
    leaving.current = true;
    await logout(queryClient);
    resetTrail();
    await navigate({ to: "/dang-nhap" });
  };

  const toggleSidebar = () => {
    setCollapsed((c) => {
      saveSidebarCollapsed(!c);
      return !c;
    });
  };

  return (
    <div className="shell">
      <SkipLink target="main" />
      {/* The focus ring switches to the bright gold: the standard one is too dark on maroon. */}
      <aside className="sidebar chrome" data-collapsed={collapsed || undefined}>
        <Link
          to="/"
          className="sidebar-brand"
          aria-label="Viên Phúc Y Đường, về trang tổng quan"
        >
          <BrandLogo className="sidebar-logo" />
        </Link>
        <nav aria-label="Chính" className="sidebar-nav">
          <NavLinks groups={groups} />
        </nav>
        <button
          type="button"
          onClick={toggleSidebar}
          className="sidebar-toggle"
          aria-label={collapsed ? "Mở rộng thanh bên" : "Thu gọn thanh bên"}
          aria-pressed={collapsed}
        >
          <Icon name={collapsed ? "chevron-right" : "chevron-left"} />
          <span className="nav-label">Thu gọn</span>
        </button>
      </aside>

      <div className="shell-main">
        <header className="topbar">
          <Link
            to="/"
            className="topbar-brand"
            aria-label="Viên Phúc Y Đường, về trang tổng quan"
          >
            <BrandLogo className="w-12" />
          </Link>
          <TodayChip canOpenCalendar={canOpenCalendar} />
          <button
            type="button"
            onClick={() => {
              setPalette(true);
            }}
            className="search-trigger"
            aria-keyshortcuts="Control+K"
            aria-haspopup="dialog"
          >
            <Icon name="search" className="size-[18px]" />
            <span className="truncate">
              <span className="sm:hidden">Tìm nhanh…</span>
              <span className="max-sm:hidden">Tìm khách, mở màn hình…</span>
            </span>
            <kbd className="ml-auto max-md:hidden">Ctrl K</kbd>
          </button>
          <UserMenu
            me={me}
            onLock={() => void lock()}
            onLogout={() => void signOut()}
          />
        </header>

        <main id="main" tabIndex={-1} className="shell-content">
          <div key={pathname} className={navigated ? "page-enter" : undefined}>
            <InShell value>
              <Outlet />
            </InShell>
          </div>
        </main>
      </div>

      <nav aria-label="Thanh điều hướng" className="tabbar chrome">
        {tabs.map((t) => (
          <Link
            key={navKey(t)}
            to={t.to}
            {...linkSearch(t)}
            activeOptions={activeFor(t)}
            className="tab"
          >
            <Icon name={t.icon} />
            <span>{t.short ?? t.label}</span>
          </Link>
        ))}
        <button
          type="button"
          className="tab"
          aria-haspopup="dialog"
          aria-label="Thêm màn hình"
          onClick={() => {
            setDrawer(true);
          }}
        >
          <Icon name="more" />
          <span>Thêm</span>
        </button>
      </nav>

      <Modal
        open={drawer}
        onClose={() => {
          setDrawer(false);
        }}
        className="drawer chrome"
        label="Điều hướng"
      >
        <div className="mb-4 flex items-center justify-between">
          <BrandLogo className="w-24 text-chrome-accent" />
          <button
            type="button"
            className="icon-btn"
            aria-label="Đóng"
            onClick={() => {
              setDrawer(false);
            }}
          >
            <Icon name="x" />
          </button>
        </div>
        <nav aria-label="Tất cả màn hình">
          <NavLinks
            groups={groups}
            onNavigate={() => {
              setDrawer(false);
            }}
          />
        </nav>
      </Modal>

      {palette && (
        <Suspense fallback={null}>
          <CommandPalette
            groups={groups}
            onClose={() => {
              setPalette(false);
            }}
          />
        </Suspense>
      )}
    </div>
  );
}
