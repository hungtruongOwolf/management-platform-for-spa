import type {
  CustomerPackage,
  CustomerPackageItem,
  CustomerSummary,
} from "@vpyd/api-client";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { messageFor } from "../../lib/errors";
import { customerQuery } from "../customers/api";
import { CustomerName } from "../../app/CustomerName";
import { HealthNote } from "../customers/HealthNote";
import {
  STATUS_BADGE,
  STATUS_LABEL,
  customerPackagesQuery,
  formatDate,
} from "../sales/api";
import { byUseOrder, isUsable } from "./api";
import { expiryText, serviceLabel } from "../../lib/names";

export interface Deduction {
  pkg: CustomerPackage;
  item: CustomerPackageItem;
  expiryOverrideReason?: string;
}

/**
 * The selected customer, inline right under the search box it was picked in (EM round 4 item 6):
 * name, phone, gender, health warning, a one-line summary of active packages, then the packages in
 * the order they should be used with one "Trừ 1 buổi" per item. Remaining counts already subtract
 * what is in the cart.
 */
export function CustomerCard({
  summary,
  inCart,
  canOverrideExpiry,
  focusFirst,
  onDeduct,
  onBlocked,
  onClear,
}: {
  summary: CustomerSummary;
  inCart: (itemId: string) => number;
  canOverrideExpiry: boolean;
  /** Move focus to the first usable "Trừ 1 buổi" once packages are shown (fast path step 1 → 2). */
  focusFirst: boolean;
  onDeduct: (d: Deduction) => void;
  onBlocked: (message: string) => void;
  /** "Đổi khách" (same as Esc). */
  onClear?: () => void;
}) {
  const customer = useQuery(customerQuery(summary.id));
  const packages = useQuery(customerPackagesQuery(summary.id));
  const firstButton = useRef<HTMLButtonElement | null>(null);
  const [overriding, setOverriding] = useState<Deduction | null>(null);
  const [reason, setReason] = useState("");

  const ordered = packages.data ? byUseOrder(packages.data) : [];
  useEffect(() => {
    if (focusFirst && packages.isSuccess) firstButton.current?.focus();
  }, [focusFirst, packages.isSuccess]);

  const firstUsableItem = ordered
    .filter(isUsable)
    .flatMap((p) => p.items)
    .find((i) => i.sessions_remaining - inCart(i.id) > 0)?.id;
  const gender = customer.data?.gender ?? summary.gender;
  const active = ordered.filter(isUsable);
  const sessionsLeft = active.reduce((n, p) => n + p.sessions_remaining, 0);

  return (
    <section
      aria-labelledby="customer-card-title"
      className="border-t border-border pt-3"
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <h2
            id="customer-card-title"
            className="font-brand text-xl text-primary"
          >
            <CustomerName
              name={summary.full_name}
              gender={gender}
              showUnknown
            />
          </h2>
          <p className="text-sm text-text-muted">
            <span className="tabular">{summary.phone}</span>
            {packages.isSuccess && (
              <>
                {" · "}
                {active.length > 0
                  ? `${String(active.length)} gói liệu trình đang dùng, còn ${String(sessionsLeft)} buổi`
                  : "Chưa có gói liệu trình đang dùng"}
              </>
            )}
          </p>
        </div>
        {onClear && (
          <button
            type="button"
            onClick={onClear}
            className="min-h-11 rounded-[10px] border border-border px-3 text-sm"
          >
            Đổi khách
          </button>
        )}
      </div>
      {customer.data && <HealthNote customer={customer.data} />}

      <h3 className="mt-3 font-semibold">Gói liệu trình</h3>
      {packages.isPending && (
        <div aria-hidden="true" className="mt-2 grid content-start gap-2">
          <div className="h-14 skeleton" />
          <div className="h-14 skeleton" />
        </div>
      )}
      {packages.isError && (
        <p role="alert" className="mt-2 text-danger">
          {messageFor(packages.error)}{" "}
          <button
            type="button"
            className="underline"
            onClick={() => void packages.refetch()}
          >
            Thử lại
          </button>
        </p>
      )}
      {packages.isSuccess && ordered.length === 0 && (
        <p className="mt-2 text-text-muted">
          Khách chưa có liệu trình. Thêm dịch vụ lẻ ở cột bên phải.
        </p>
      )}
      <ul aria-label="Liệu trình của khách" className="mt-2 grid gap-2">
        {ordered.flatMap((p) =>
          p.items.map((item) => {
            const left = item.sessions_remaining - inCart(item.id);
            const expired = p.status === "expired";
            const blocked =
              p.status === "cancelled" ||
              left <= 0 ||
              (expired && !canOverrideExpiry);
            const isFirst = item.id === firstUsableItem;
            const label =
              p.items.length > 1
                ? `${p.name} · ${serviceLabel(item.name, item.category_name)}`
                : p.name;
            return (
              <li
                key={item.id}
                className={`flex flex-wrap items-center justify-between gap-2 rounded-[10px] border border-border border-l-4 px-3 py-2 ${blocked ? "border-l-border bg-surface-2" : "border-l-accent"}`}
              >
                <div>
                  <span className="font-semibold">{label}</span>
                  {/* Sessions left as a gold bar: readable at a glance; the numbers below say it. */}
                  <span
                    aria-hidden="true"
                    className="mt-1 block h-1.5 w-40 max-w-full overflow-hidden rounded-full bg-surface-2"
                  >
                    <span
                      className={`block h-full rounded-full ${left <= 2 ? "bg-danger" : "bg-accent"}`}
                      style={{
                        width: `${String(Math.round((Math.max(left, 0) / Math.max(item.sessions_total, 1)) * 100))}%`,
                      }}
                    />
                  </span>
                  <span className="block text-sm text-text-muted">
                    Còn{" "}
                    <strong>
                      {String(Math.max(left, 0))}/{String(item.sessions_total)}
                    </strong>{" "}
                    buổi · {expiryText(p.expires_on, formatDate)}
                    {p.status !== "active" && (
                      <span
                        className={`badge ml-2 align-middle ${STATUS_BADGE[p.status]}`}
                      >
                        {STATUS_LABEL[p.status]}
                      </span>
                    )}
                  </span>
                  {item.sessions_remaining === 0 && (
                    <span className="block text-sm text-danger">
                      Đã hết buổi. Gợi ý: tư vấn bán gói mới.
                    </span>
                  )}
                </div>
                <button
                  type="button"
                  ref={isFirst ? firstButton : undefined}
                  aria-disabled={blocked}
                  aria-label={`Trừ 1 buổi ${label}`}
                  onClick={() => {
                    if (p.status === "cancelled") {
                      onBlocked("Liệu trình đã bị hủy, không trừ được.");
                      return;
                    }
                    if (left <= 0) {
                      onBlocked(
                        item.sessions_remaining === 0
                          ? "Liệu trình đã hết buổi. Gợi ý bán gói mới."
                          : "Đã trừ hết số buổi còn lại trong lượt này.",
                      );
                      return;
                    }
                    if (expired) {
                      if (!canOverrideExpiry) {
                        onBlocked(
                          "Liệu trình đã hết hạn. Cần quản lý cho dùng tiếp.",
                        );
                        return;
                      }
                      setReason("");
                      setOverriding({ pkg: p, item });
                      return;
                    }
                    onDeduct({ pkg: p, item });
                  }}
                  className="min-h-11 rounded-[10px] border border-primary bg-surface px-4 font-semibold text-primary not-aria-disabled:hover:bg-primary-soft aria-disabled:cursor-not-allowed aria-disabled:opacity-55"
                >
                  Trừ 1 buổi
                </button>
              </li>
            );
          }),
        )}
      </ul>

      {overriding && (
        <form
          aria-label="Cho dùng tiếp liệu trình hết hạn"
          className="mt-3 grid content-start gap-2 rounded-[10px] border border-warning-border bg-warning-bg p-3 text-warning-text"
          onSubmit={(e) => {
            e.preventDefault();
            if (reason.trim() === "") return;
            onDeduct({ ...overriding, expiryOverrideReason: reason.trim() });
            setOverriding(null);
          }}
        >
          <label className="block">
            <span className="text-sm font-semibold">
              Lý do cho dùng tiếp “{overriding.pkg.name}” (đã hết hạn)
            </span>
            <input
              autoFocus
              value={reason}
              onChange={(e) => {
                setReason(e.target.value);
              }}
              className="min-h-11 w-full rounded-[6px] border border-border bg-surface px-3 text-text"
            />
          </label>
          <div className="flex gap-2">
            <button
              type="submit"
              aria-disabled={reason.trim() === ""}
              className="min-h-11 rounded-[10px] bg-primary px-4 font-semibold text-on-primary aria-disabled:opacity-60"
            >
              Cho dùng tiếp
            </button>
            <button
              type="button"
              onClick={() => {
                setOverriding(null);
              }}
              className="min-h-11 rounded-[10px] border border-border bg-surface px-4 text-text"
            >
              Thôi
            </button>
          </div>
        </form>
      )}
    </section>
  );
}
