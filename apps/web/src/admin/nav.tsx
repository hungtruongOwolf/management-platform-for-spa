import { createContext, useCallback, useContext, useEffect, useState, type MouseEvent, type ReactNode } from "react";

/**
 * The console's own few routes, on the History API. Not a second TanStack router: the spa's router
 * type is the registered one, and the console needs only paths (no search state, no loaders).
 */
interface Nav {
  path: string;
  go: (path: string) => void;
}

const NavContext = createContext<Nav | null>(null);

export function AdminNavProvider({ children }: { children: ReactNode }) {
  const [path, setPath] = useState(() => window.location.pathname);
  useEffect(() => {
    const onPop = () => {
      setPath(window.location.pathname);
    };
    window.addEventListener("popstate", onPop);
    return () => {
      window.removeEventListener("popstate", onPop);
    };
  }, []);
  const go = useCallback((next: string) => {
    if (next !== window.location.pathname) window.history.pushState(null, "", next);
    setPath(next);
    window.scrollTo({ top: 0 });
  }, []);
  return <NavContext.Provider value={{ path, go }}>{children}</NavContext.Provider>;
}

export function useAdminNav(): Nav {
  const nav = useContext(NavContext);
  if (!nav) throw new Error("useAdminNav outside AdminNavProvider");
  return nav;
}

/** A plain link that navigates inside the console (new tab / modifier keys keep working). */
export function AdminLink({
  to,
  className,
  children,
  current,
}: {
  to: string;
  className?: string;
  children: ReactNode;
  current?: boolean;
}) {
  const { go } = useAdminNav();
  const onClick = (e: MouseEvent<HTMLAnchorElement>) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    e.preventDefault();
    go(to);
  };
  return (
    <a href={to} className={className} onClick={onClick} aria-current={current ? "page" : undefined}>
      {children}
    </a>
  );
}

export type AdminRoute =
  | { page: "login" }
  | { page: "tenants" }
  | { page: "tenant"; id: string }
  | { page: "audit" }
  | { page: "not_found" };

export function matchAdminRoute(path: string): AdminRoute {
  const trimmed = path.replace(/\/+$/, "") || "/";
  if (trimmed === "/admin") return { page: "tenants" };
  if (trimmed === "/admin/dang-nhap") return { page: "login" };
  if (trimmed === "/admin/nhat-ky") return { page: "audit" };
  const spa = /^\/admin\/spa\/([0-9a-f-]{36})$/.exec(trimmed);
  if (spa?.[1]) return { page: "tenant", id: spa[1] };
  return { page: "not_found" };
}
