import { useRouter, useRouterState } from "@tanstack/react-router";
import { useEffect, type MouseEvent, type ReactNode } from "react";
import { Icon } from "./Icon";
import { screenLabel } from "./nav";
import { lastHrefFor, previousEntry, setTitle } from "./trail";

/** One level above the current page, e.g. `{ label: "Khách hàng", to: "/khach-hang" }`. */
export interface Crumb {
  label: string;
  /** Path of the parent screen; opening it restores the list state this tab last had there. */
  to: string;
}

export interface PageHeaderProps {
  title: ReactNode;
  /** Plain-text title for the trail ("‹ Quay lại …") when `title` is not a string. */
  label?: string;
  /** Small gold line above the title (section name on top-level screens). */
  eyebrow?: string;
  /** Ancestors, root first; the current page is appended automatically. */
  breadcrumbs?: Crumb[];
  /** Show the back control. Default: when there are breadcrumbs (detail pages). */
  back?: boolean;
  /** Id for the h1, when a landmark is labelled by it. */
  titleId?: string;
  /** Buttons or controls on the right (wrap below the title on narrow screens). */
  children?: ReactNode;
}

/** A plain left click: let modified clicks (new tab, new window) keep their browser meaning. */
function plainClick(e: MouseEvent): boolean {
  return e.button === 0 && !e.metaKey && !e.ctrlKey && !e.shiftKey && !e.altKey;
}

/**
 * Header of every screen (design-system.md "Điều hướng"): optional eyebrow, breadcrumbs that show
 * where the page sits (Khách hàng › Nguyễn Thị Hà), a back control that returns to where the user
 * actually came from — the previous in-app screen with its filters and scroll, through the browser
 * history — or, after a deep link, to the logical parent. It also records its title in the trail so
 * the next screen can name it.
 */
export function PageHeader({ title, label, eyebrow, breadcrumbs = [], back, titleId, children }: PageHeaderProps) {
  const router = useRouter();
  const { index, href, pathname } = useRouterState({
    select: (s) => ({ index: s.location.state.__TSR_index, href: s.location.href, pathname: s.location.pathname }),
    structuralSharing: true,
  });
  const plain = label ?? (typeof title === "string" ? title : undefined);

  useEffect(() => {
    if (plain) setTitle(index, href, pathname, plain);
  }, [index, href, pathname, plain]);

  const parent = breadcrumbs.at(-1);
  const showBack = back ?? breadcrumbs.length > 0;
  const prev = showBack ? previousEntry(index, pathname) : undefined;
  const crumbHref = (c: Crumb) => lastHrefFor(c.to, index) ?? c.to;
  const backHref = prev?.href ?? (parent ? crumbHref(parent) : undefined);
  const backLabel = prev ? (prev.title ?? screenLabel(prev.pathname)) : parent?.label;
  // Back to the parent says the same as the breadcrumbs: from 640px only the breadcrumbs show.
  const backIsParent = parent !== undefined && (!prev || prev.pathname === parent.to);

  const go = (e: MouseEvent, target: string, useHistory: boolean) => {
    if (!plainClick(e)) return;
    e.preventDefault();
    if (useHistory) router.history.back();
    else void router.navigate({ href: target });
  };

  return (
    <header className="page-header">
      {(showBack && backHref) || breadcrumbs.length > 0 ? (
        <div className="page-header-nav">
          {showBack && backHref && (
            <a
              href={backHref}
              className={backIsParent ? "back-link back-link-parent" : "back-link"}
              onClick={(e) => {
                go(e, backHref, prev !== undefined);
              }}
            >
              <Icon name="chevron-left" className="size-4" />
              <span className="truncate">
                <span className="sr-only">Quay lại</span>{" "}
                {backLabel ?? "Trang trước"}
              </span>
            </a>
          )}
          {breadcrumbs.length > 0 && (
            <nav aria-label="Vị trí trang" className="breadcrumbs">
              <ol>
                {breadcrumbs.map((c) => (
                  <li key={c.to}>
                    <a
                      href={crumbHref(c)}
                      onClick={(e) => {
                        go(e, crumbHref(c), false);
                      }}
                    >
                      {c.label}
                    </a>
                  </li>
                ))}
                <li aria-current="page">{plain ?? title}</li>
              </ol>
            </nav>
          )}
        </div>
      ) : null}
      <div className="page-header-main">
        <div className="min-w-0">
          {eyebrow && <p className="eyebrow">{eyebrow}</p>}
          <h1 id={titleId} className="page-title">
            {title}
          </h1>
        </div>
        {children && <div className="page-header-actions">{children}</div>}
      </div>
    </header>
  );
}
