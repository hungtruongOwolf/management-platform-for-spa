import type {
  CustomerPackage,
  CustomerPackageItem,
  CustomerSummary,
} from "@vpyd/api-client";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { messageFor, retryable } from "../../lib/errors";
import { customerQuery } from "../customers/api";
import { CustomerName } from "../../app/CustomerName";
import { Icon } from "../../app/Icon";
import { HealthNote } from "../customers/HealthNote";
import { customerPackagesQuery, formatDate } from "../sales/api";
import { byUseOrder, isUsable } from "./api";
import { expiryText, serviceLabel } from "../../lib/names";

export interface Deduction {
  pkg: CustomerPackage;
  item: CustomerPackageItem;
  expiryOverrideReason?: string;
}

interface Row {
  pkg: CustomerPackage;
  item: CustomerPackageItem;
  label: string;
  /** Sessions left after what this cart already uses. */
  left: number;
}

/**
 * One state per row (design-system.md "Quy tắc"): a package item is open (in use, or expired with
 * sessions left) or closed (cancelled, or no session left). Closed rows never offer "Trừ 1 buổi";
 * they wait behind the "Gói đã hết / đã hủy" disclosure.
 */
function isClosed(p: CustomerPackage, item: CustomerPackageItem): boolean {
  return (
    p.status === "cancelled" ||
    p.status === "exhausted" ||
    item.sessions_remaining <= 0
  );
}

/**
 * The selected customer, inline right under the search box it was picked in (EM round 4 item 6):
 * name, phone, gender, health warning, a one-line summary of active packages, then the packages in
 * the order they should be used with one "Trừ 1 buổi" per open item. Remaining counts already
 * subtract what is in the cart.
 */
export function CustomerCard({
  summary,
  inCart,
  canOverrideExpiry,
  focusFirst,
  onDeduct,
  onBlocked,
  onClear,
  onSellPackage,
  onEditInfo,
}: {
  /** "Thêm thông tin khách" on a nameless walk-in: edit the profile in place (ADR-0011). */
  onEditInfo?: () => void;
  summary: CustomerSummary;
  inCart: (itemId: string) => number;
  canOverrideExpiry: boolean;
  /** Move focus to the first usable "Trừ 1 buổi" once packages are shown (fast path step 1 → 2). */
  focusFirst: boolean;
  onDeduct: (d: Deduction) => void;
  onBlocked: (message: string) => void;
  /** "Đổi khách" (same as Esc). */
  onClear?: () => void;
  /**
   * "Mua lại gói này" (that plan): opens a prefilled package block in the cart (spec §10.4.1,
   * US10). Selling a new package starts in the cart only (one entry point, EM 2026-11-07).
   * Absent without `package.sell`.
   */
  onSellPackage?: (planId: string | null) => void;
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

  const rows: Row[] = ordered.flatMap((p) =>
    p.items.map((item) => ({
      pkg: p,
      item,
      label:
        p.items.length > 1
          ? `${p.name} · ${serviceLabel(item.name, item.category_name)}`
          : p.name,
      left: item.sessions_remaining - inCart(item.id),
    })),
  );
  const open = rows.filter((r) => !isClosed(r.pkg, r.item));
  const closed = rows.filter((r) => isClosed(r.pkg, r.item));
  const firstUsableItem = open.find((r) => isUsable(r.pkg) && r.left > 0)?.item
    .id;
  const gender = customer.data?.gender ?? summary.gender;
  const active = ordered.filter(isUsable);
  const sessionsLeft = active.reduce((n, p) => n + p.sessions_remaining, 0);

  const deduct = ({ pkg: p, item, left }: Row) => {
    if (left <= 0) {
      onBlocked("Đã trừ hết số buổi còn lại trong lượt này.");
      return;
    }
    if (p.status === "expired") {
      setReason("");
      setOverriding({ pkg: p, item });
      return;
    }
    onDeduct({ pkg: p, item });
  };

  return (
    <section aria-labelledby="customer-card-title" className="grid gap-4">
      <div className="customer-head">
        <div className="min-w-0">
          <h2 id="customer-card-title" className="customer-title">
            <CustomerName
              name={summary.full_name}
              gender={gender}
              showUnknown
            />
          </h2>
          <p className="customer-meta">
            {[
              // A nameless walk-in has no phone to speak of: say nothing rather than "Chưa có".
              summary.phone ?? (summary.named ? "Chưa có SĐT" : null),
              packages.isSuccess
                ? active.length > 0
                  ? `${String(active.length)} gói liệu trình đang dùng, còn ${String(sessionsLeft)} buổi`
                  : "Chưa có gói liệu trình đang dùng"
                : null,
            ]
              .filter(Boolean)
              .join(" · ")}
          </p>
        </div>
        {onClear && (
          <button type="button" onClick={onClear} className="btn btn-secondary">
            Đổi khách
          </button>
        )}
      </div>
      {onEditInfo && !summary.named && (
        <button
          type="button"
          onClick={onEditInfo}
          className="btn btn-secondary justify-self-start"
        >
          <Icon name="plus" />
          Thêm thông tin khách
        </button>
      )}
      {customer.data && <HealthNote customer={customer.data} />}

      <div className="grid gap-2">
        <h3 className="section-label">Gói liệu trình</h3>
        {packages.isPending && (
          <div aria-hidden="true" className="grid content-start gap-2">
            <div className="h-14 skeleton" />
            <div className="h-14 skeleton" />
          </div>
        )}
        {packages.isError && (
          <p role="alert" className="field-error">
            {messageFor(packages.error)}{" "}
            {retryable(packages.error) && (
              <button
                type="button"
                className="btn btn-ghost min-h-9 px-2"
                onClick={() => void packages.refetch()}
              >
                Thử lại
              </button>
            )}
          </p>
        )}
        {packages.isSuccess && ordered.length === 0 && (
          <p className="text-text-muted">
            Khách chưa có gói liệu trình. Chọn dịch vụ lẻ hoặc bán gói ở phần
            “Lượt khách đang ghi”.
          </p>
        )}
        {packages.isSuccess && ordered.length > 0 && open.length === 0 && (
          <p className="text-text-muted">
            Không còn gói nào dùng được. Gợi ý: tư vấn bán gói mới.
          </p>
        )}
        <ul aria-label="Liệu trình của khách" className="pkg-list">
          {open.map((r) => {
            const { pkg: p, item, label, left } = r;
            const expired = p.status === "expired";
            const isFirst = item.id === firstUsableItem;
            return (
              <li key={item.id} className="pkg-row">
                <div className="grid min-w-0 gap-1">
                  <span className="pkg-name">{label}</span>
                  <span className="pkg-meta">
                    <span>
                      Còn{" "}
                      <strong>
                        {String(Math.max(left, 0))}/
                        {String(item.sessions_total)}
                      </strong>{" "}
                      buổi
                    </span>
                    <span aria-hidden="true">·</span>
                    <span>{expiryText(p.expires_on, formatDate)}</span>
                    {expired ? (
                      <span className="badge badge-warn">Hết hạn</span>
                    ) : (
                      left > 0 &&
                      left <= 2 && (
                        <span className="badge badge-warn">Sắp hết buổi</span>
                      )
                    )}
                  </span>
                </div>
                {expired && !canOverrideExpiry ? (
                  <span className="pkg-note">
                    Đã hết hạn: cần quản lý cho dùng tiếp.
                  </span>
                ) : (
                  <button
                    type="button"
                    ref={isFirst ? firstButton : undefined}
                    aria-disabled={left <= 0}
                    aria-label={`Trừ 1 buổi ${label}`}
                    onClick={() => {
                      deduct(r);
                    }}
                    className="btn btn-secondary"
                  >
                    Trừ 1 buổi
                  </button>
                )}
                {left <= 0 && (
                  <span className="pkg-note">
                    Đã trừ hết số buổi còn lại trong lượt này.
                  </span>
                )}
              </li>
            );
          })}
        </ul>

        {overriding && (
          <form
            aria-label="Cho dùng tiếp liệu trình hết hạn"
            className="callout"
            onSubmit={(e) => {
              e.preventDefault();
              if (reason.trim() === "") return;
              onDeduct({ ...overriding, expiryOverrideReason: reason.trim() });
              setOverriding(null);
            }}
          >
            <label className="field">
              <span>
                Lý do cho dùng tiếp “{overriding.pkg.name}” (đã hết hạn)
              </span>
              <input
                autoFocus
                value={reason}
                onChange={(e) => {
                  setReason(e.target.value);
                }}
                className="input"
              />
            </label>
            <div className="button-row">
              <button
                type="submit"
                aria-disabled={reason.trim() === ""}
                className="btn btn-primary"
              >
                Cho dùng tiếp
              </button>
              <button
                type="button"
                onClick={() => {
                  setOverriding(null);
                }}
                className="btn btn-secondary"
              >
                Thôi
              </button>
            </div>
          </form>
        )}

        {closed.length > 0 && (
          <details className="disclosure">
            <summary>
              Gói đã hết / đã hủy ({closed.length})
            </summary>
            <ul aria-label="Gói đã hết hoặc đã hủy" className="pkg-list">
              {closed.map(({ pkg: p, item, label }) => (
                <li key={item.id} className="pkg-row">
                  <div className="grid min-w-0 gap-1">
                    <span className="pkg-name">{label}</span>
                    <span className="pkg-meta">
                      <span>
                        Đã dùng{" "}
                        <strong>
                          {String(
                            item.sessions_total - item.sessions_remaining,
                          )}
                          /{String(item.sessions_total)}
                        </strong>{" "}
                        buổi
                      </span>
                      <span aria-hidden="true">·</span>
                      <span>{expiryText(p.expires_on, formatDate)}</span>
                    </span>
                  </div>
                  {p.status === "cancelled" ? (
                    <span className="badge badge-danger">Đã hủy</span>
                  ) : (
                    <span className="badge badge-muted">Hết buổi</span>
                  )}
                  {onSellPackage && p.status !== "cancelled" && (
                    <button
                      type="button"
                      aria-label={`Mua lại gói này: ${p.name}`}
                      className="btn btn-ghost pkg-rebuy"
                      onClick={() => {
                        onSellPackage(p.plan_id ?? null);
                      }}
                    >
                      Mua lại gói này
                    </button>
                  )}
                </li>
              ))}
            </ul>
          </details>
        )}
      </div>
    </section>
  );
}
