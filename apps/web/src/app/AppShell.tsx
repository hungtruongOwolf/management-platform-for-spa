import { useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { Link, Outlet, useNavigate, useRouterState } from "@tanstack/react-router";
import { Suspense, lazy, useEffect, useRef, useState, type ReactNode } from "react";
import { logout, meQuery } from "../features/auth/session";
import { BrandMark, BrandWordmark } from "./BrandMark";
import { Icon } from "./Icon";
import { navFor, tabsFor, type NavGroup } from "./nav";
import { readSidebarCollapsed, saveSidebarCollapsed } from "./prefs";

// Opens on Ctrl/⌘+K or the search button only: its own chunk, off the startup path.
const CommandPalette = lazy(() => import("./CommandPalette"));

const longDate = new Intl.DateTimeFormat("vi-VN", {
  weekday: "long",
  day: "numeric",
  month: "numeric",
  year: "numeric",
  timeZone: "Asia/Ho_Chi_Minh",
});

/** Given name's initial: Vietnamese names end with it. */
export function initialOf(name: string): string {
  return name.trim().split(/\s+/).at(-1)?.charAt(0).toUpperCase() ?? "";
}

function NavLinks({ groups, onNavigate }: { groups: NavGroup[]; onNavigate?: () => void }) {
  return (
    <>
      {groups.map((g) => (
        <div key={g.label} role="group" aria-labelledby={`nav-${g.label}`} className="nav-group">
          <p id={`nav-${g.label}`} className="nav-group-label">
            {g.label}
          </p>
          {g.items.map((item) => (
            <Link
              key={item.to}
              to={item.to}
              activeOptions={{ exact: item.exact ?? false }}
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
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) {
      if (typeof d.showModal === "function") d.showModal();
      else d.setAttribute("open", "");
    } else if (!open && d.open) {
      if (typeof d.close === "function") d.close();
      else d.removeAttribute("open");
    }
  }, [open]);
  return (
    <dialog
      ref={ref}
      aria-label={label}
      className={className}
      onClose={onClose}
      // A click on the backdrop lands on the dialog element itself.
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      {open && children}
    </dialog>
  );
}

/**
 * Signed-in frame. Desktop/tablet: a maroon sidebar (logo, grouped nav, collapsible on ≥ 1024px,
 * an icon rail below that) and a top bar (date, search / command palette, user, quick lock).
 * Phones: a top bar and a bottom tab bar with a "Thêm" drawer. Nav is filtered by permission (UX
 * only — the API decides).
 */
export function AppShell() {
  const { data: me } = useSuspenseQuery(meQuery);
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const groups = navFor(me);
  const tabs = tabsFor(groups);
  const [collapsed, setCollapsed] = useState(readSidebarCollapsed);
  const [palette, setPalette] = useState(false);
  const [drawer, setDrawer] = useState(false);
  const pathname = useRouterState({ select: (s) => s.location.pathname });

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

  // "Khóa nhanh": leave the counter safely in one tap.
  const lock = async () => {
    await logout(queryClient);
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
      {/* The focus ring switches to the bright gold: the standard one is too dark on maroon. */}
      <aside className="sidebar chrome" data-collapsed={collapsed || undefined}>
        <Link to="/" className="sidebar-brand" aria-label="Viên Phúc Y Đường, về trang tổng quan">
          <BrandMark className="size-10 shrink-0" />
          <BrandWordmark className="sidebar-wordmark h-9 w-[70px]" />
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
          <Link to="/" className="topbar-brand" aria-label="Viên Phúc Y Đường, về trang tổng quan">
            <BrandMark className="size-8 text-primary" />
          </Link>
          <p className="hidden text-sm text-text-muted first-letter:uppercase md:block">
            {longDate.format(new Date())}
          </p>
          <button
            type="button"
            onClick={() => {
              setPalette(true);
            }}
            className="search-trigger"
            aria-label="Tìm khách hoặc mở màn hình (Ctrl+K)"
            aria-haspopup="dialog"
          >
            <Icon name="search" className="size-[18px]" />
            <span className="max-sm:sr-only">Tìm khách, mở màn hình…</span>
            <kbd className="ml-auto max-md:hidden">Ctrl K</kbd>
          </button>
          <span className="user-chip">
            <span aria-hidden="true" className="avatar">
              {initialOf(me?.user.full_name ?? "")}
            </span>
            <span className="leading-tight max-xl:hidden">
              <span className="block text-sm font-semibold">{me?.user.full_name}</span>
              <span className="block text-xs text-text-muted">
                {me?.active_tenant?.name ?? "Chưa chọn spa"}
              </span>
            </span>
          </span>
          <button type="button" onClick={() => void lock()} className="btn btn-secondary shrink-0" aria-label="Khóa nhanh">
            <Icon name="lock" className="size-[18px]" />
            <span className="max-sm:hidden">Khóa nhanh</span>
          </button>
        </header>

        <main id="main" className="shell-content">
          <div key={pathname} className={navigated ? "page-enter" : undefined}>
            <Outlet />
          </div>
        </main>
      </div>

      <nav aria-label="Thanh điều hướng" className="tabbar chrome">
        {tabs.map((t) => (
          <Link key={t.to} to={t.to} activeOptions={{ exact: t.exact ?? false }} className="tab">
            <Icon name={t.icon} />
            <span>{t.to === "/" ? "Trang chủ" : t.label.replace("Ghi lượt khách", "Quầy").replace("Hồ sơ khách", "Khách")}</span>
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
          <span className="flex items-center gap-2">
            <BrandMark className="size-10" />
            <BrandWordmark className="h-9 w-[70px]" />
          </span>
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
