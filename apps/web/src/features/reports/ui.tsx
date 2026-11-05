import { formatVnd } from "@vpyd/ui";
import { useSuspenseQuery } from "@tanstack/react-query";
import { useEffect, useState, type ReactNode } from "react";
import { canSeeManagement } from "../../app/features";
import { meQuery } from "../auth/session";
import { BrandLogo } from "../../app/BrandLogo";
import { Icon, type IconName } from "../../app/Icon";
import { messageFor } from "../../lib/errors";

function prefersReducedMotion(): boolean {
  return typeof window.matchMedia !== "function" || window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * A number that counts up to its value on first show (700 ms, ease-out). Text only changes, so no
 * layout work beyond the digits (tabular-nums keeps their width). Reduced motion: the value at once.
 */
export function CountUp({ value, format = (n) => n.toLocaleString("vi-VN") }: { value: number; format?: (n: number) => string }) {
  const [still] = useState(prefersReducedMotion);
  const [shown, setShown] = useState(0);
  useEffect(() => {
    if (still) return;
    let frame = 0;
    const start = performance.now();
    const tick = (now: number) => {
      const t = Math.min(1, (now - start) / 700);
      setShown(Math.round(value * (1 - (1 - t) ** 3)));
      if (t < 1) frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(frame);
    };
  }, [value, still]);
  return (
    <>
      <span aria-hidden="true">{format(still ? value : shown)}</span>
      <span className="sr-only">{format(value)}</span>
    </>
  );
}

export const money = (n: number) => formatVnd(n);

/** Change against a reference day: ▲ 12% / ▼ 3% / = ; colour carries tone, the text carries meaning. */
export function Delta({ now, before, label }: { now: number; before: number; label: string }) {
  if (before === 0 && now === 0) return <span className="text-xs text-text-muted">= {label}</span>;
  const pct = before === 0 ? 100 : Math.round(((now - before) / before) * 100);
  const up = pct > 0;
  const tone = pct === 0 ? "text-text-muted" : up ? "text-success" : "text-danger";
  return (
    <span className={`inline-flex items-center gap-1 text-xs font-semibold ${tone}`}>
      {pct !== 0 && <Icon name={up ? "arrow-up" : "arrow-down"} className="size-3.5" />}
      {pct === 0 ? "=" : `${up ? "+" : ""}${String(pct)}%`}
      <span className="font-normal text-text-muted">{label}</span>
    </span>
  );
}

export function StatCard({
  label,
  icon,
  children,
  foot,
  accent = false,
}: {
  label: string;
  icon: IconName;
  children: ReactNode;
  foot?: ReactNode;
  accent?: boolean;
}) {
  return (
    <div className={`card stat-card ${accent ? "border-transparent bg-chrome bg-[linear-gradient(135deg,var(--chrome-bg-2),var(--chrome-bg))] text-chrome-text" : ""}`}>
      <p className={`flex items-center gap-2 text-sm font-medium ${accent ? "text-chrome-text-muted" : "text-text-muted"}`}>
        <span
          aria-hidden="true"
          className={`grid size-7 place-items-center rounded-full ${accent ? "bg-[var(--chrome-hover)] text-chrome-accent" : "bg-accent-soft text-accent-text"}`}
        >
          <Icon name={icon} className="size-4" />
        </span>
        {label}
      </p>
      <p className={`stat-value tabular ${accent ? "font-brand text-chrome-accent" : "text-text"}`}>{children}</p>
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
          <div className="skeleton h-7 w-32" />
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

export function EmptyState({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-2 px-6 py-8 text-center">
      <BrandLogo className="mb-1 w-14 opacity-80" />
      <p className="font-brand text-lg text-primary">{title}</p>
      {children && <p className="max-w-sm text-sm text-text-muted">{children}</p>}
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

export function PageHeader({ eyebrow, title, children }: { eyebrow: string; title: string; children?: ReactNode }) {
  return (
    <header className="flex flex-wrap items-end justify-between gap-3">
      <div>
        <p className="eyebrow">{eyebrow}</p>
        <h1 className="page-title">{title}</h1>
      </div>
      {children}
    </header>
  );
}

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
