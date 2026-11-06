import { formatVnd } from "@vpyd/ui";
import { useSuspenseQuery } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { canSeeManagement } from "../../app/features";
import { meQuery } from "../auth/session";
import { Icon, type IconName } from "../../app/Icon";
import { messageFor } from "../../lib/errors";

export const money = (n: number) => formatVnd(n);

/** Change against a reference day: ▲ 12% / ▼ 3% / = ; colour carries tone, the text carries meaning. */
export function Delta({ now, before, label, onChrome = false }: { now: number; before: number; label: string; onChrome?: boolean }) {
  const muted = onChrome ? "text-chrome-text-muted" : "text-text-muted";
  if (before === 0 && now === 0) return <span className={`text-xs ${muted}`}>= {label}</span>;
  const pct = before === 0 ? 100 : Math.round(((now - before) / before) * 100);
  const up = pct > 0;
  // On the maroon card green/red fail contrast: the arrow and sign carry the direction there.
  const tone = pct === 0 ? muted : onChrome ? "text-chrome-accent" : up ? "text-success" : "text-danger";
  return (
    <span className={`inline-flex flex-wrap items-center gap-1 text-xs font-semibold ${tone}`}>
      {pct !== 0 && <Icon name={up ? "arrow-up" : "arrow-down"} className="size-3.5" />}
      {pct === 0 ? "=" : `${up ? "+" : ""}${String(pct)}%`}
      <span className={`font-normal ${muted}`}>{label}</span>
    </span>
  );
}

/**
 * One headline number (rulebook: work area stays quiet). Label + a muted icon, the number at
 * 24px, an optional footnote. `accent` is kept for callers but no longer paints a maroon tile:
 * brand colour lives in the app chrome, and "Thực thu" leads by position (first), not by fill.
 */
export function StatCard({
  label,
  icon,
  children,
  foot,
}: {
  label: string;
  icon: IconName;
  children: ReactNode;
  foot?: ReactNode;
  accent?: boolean;
}) {
  return (
    <div className="card stat-card">
      <p className="stat-label">
        <Icon name={icon} className="size-4" />
        {label}
      </p>
      <p className="stat-value tabular">{children}</p>
      {/* Cards in a row stretch to the tallest (grid), so a footnote only takes space where it exists. */}
      {foot !== undefined && <div className="mt-auto">{foot}</div>}
    </div>
  );
}

/** Placeholder cards inside a stat grid; the first one announces the loading state. */
export function StatSkeleton({ count = 4 }: { count?: number }) {
  return (
    <>
      {Array.from({ length: count }, (_, i) => (
        <div key={i} className="card stat-card" {...(i === 0 ? { role: "status", "aria-label": "Đang tải số liệu" } : {})}>
          <div className="skeleton h-[18px] w-28" />
          <div className="skeleton h-8 w-24" />
        </div>
      ))}
    </>
  );
}

export function RowsSkeleton({ rows = 5 }: { rows?: number }) {
  return (
    <div role="status" aria-label="Đang tải" className="grid content-start gap-3 p-4">
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="skeleton h-10" />
      ))}
    </div>
  );
}

/** Loading wrapper: announces once to screen readers, shows the skeleton to everyone else. */
export function Loading({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div role="status" aria-label={label}>
      {children}
    </div>
  );
}

/**
 * Empty list or panel (rulebook §9): what is missing, then the next step in words, and the button
 * for it when there is one (`action`). A muted icon, never the logo (brand lives in the chrome).
 */
export function EmptyState({
  title,
  children,
  icon = "sparkle",
  action,
}: {
  title: string;
  children?: ReactNode;
  icon?: IconName;
  action?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <span aria-hidden="true" className="empty-state-icon">
        <Icon name={icon} className="size-5" />
      </span>
      <p className="empty-state-title">{title}</p>
      {children && <p className="max-w-sm text-sm text-text-muted">{children}</p>}
      {action}
    </div>
  );
}

export function ErrorState({ error, onRetry }: { error: unknown; onRetry: () => void }) {
  return (
    <div role="alert" className="flex flex-wrap items-center gap-3 rounded-[10px] bg-danger-soft p-4 text-danger">
      <Icon name="alert" />
      <span className="flex-1">{messageFor(error)}</span>
      <button type="button" className="btn btn-secondary" onClick={onRetry}>
        Thử lại
      </button>
    </div>
  );
}

/** The app-wide page header (breadcrumbs, back, title, actions): app/PageHeader.tsx. */
export { PageHeader } from "../../app/PageHeader";

/** UX gate of the management screens (the API answers 403 by itself). */
export function ManagementOnly({ children }: { children: ReactNode }) {
  const { data: me } = useSuspenseQuery(meQuery);
  if (!canSeeManagement(me)) {
    return (
      <p role="alert" className="rounded-[10px] bg-surface-2 p-4 text-text-muted">
        Màn hình này chưa mở cho tài khoản của bạn.
      </p>
    );
  }
  return <>{children}</>;
}

/** A count as shown on the stat tiles (vi-VN digit grouping). */
export const count = (n: number) => n.toLocaleString("vi-VN");
