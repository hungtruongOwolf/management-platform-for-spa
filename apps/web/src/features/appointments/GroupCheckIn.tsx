import type { Appointment, Calendar } from "@vpyd/api-client";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import { useState } from "react";
import { CustomerName } from "../../app/CustomerName";
import { BottomSheet } from "../../components/BottomSheet";
import { messageFor } from "../../lib/errors";
import { invalidateManagementViews } from "../../lib/invalidate";
import { spaToday } from "../corrections/api";
import {
  type CartLine,
  type PackageSaleDraft,
  amountDue,
  coverage,
  packagesDue,
  paymentsFor,
  visitBody,
} from "../front-desk/api";
import { packageSalesBody } from "../front-desk/checkout";
import { GroupPackagePick } from "../front-desk/GroupPackagePick";
import { cartFromAppointment } from "../front-desk/prefill";
import { can, meQuery } from "../auth/session";
import { activeStaffQuery } from "../sales/api";
import { hhmm, spaMinutes } from "./api";
import { completeAppointment } from "./complete";

type Mode = "cash" | "bank_transfer";

interface Row {
  staffId: string;
  mode: Mode;
  /** One key per person, kept across retries: a resend never records a visit twice. */
  key: string;
  state: "ready" | "saving" | "done" | "error";
  error?: string | undefined;
  /** A package this person buys in their checkout (round 6), or null. */
  sale?: PackageSaleDraft | null;
}

/** What one person pays: their one-off lines (minus those their new package covers) + the package. */
function dueOf(lines: CartLine[], sale: PackageSaleDraft | null | undefined) {
  const sales = sale?.plan ? [sale] : [];
  const covered = coverage(lines, sales);
  return {
    sales,
    covered,
    due: amountDue(lines, covered) + packagesDue(sales),
  };
}

/** Why a member cannot be recorded from the group table (they are recorded on their own). */
export function skipReason(
  a: Appointment,
  lines: CartLine[] | undefined,
): string | null {
  if (a.is_guest) return "Khách mới: mở lịch của người này để tạo hồ sơ trước.";
  if (lines && lines.length < a.services.length)
    return "Có dịch vụ cần chọn giá theo giới tính: ghi riêng cho người này.";
  return null;
}

/**
 * "Khách đến cả nhóm" (spec appointments.md §10.3): every open member of a group, each with the
 * cart their booking implies (package session when they have one, else the listed price), a
 * technician and a payment method → one button records them all. Each person is still their own
 * visit and payment (`POST /appointments/{id}/complete`, its own Idempotency-Key); one refusal
 * stops nobody else, and pressing again resends only those not yet recorded.
 */
export function GroupCheckIn({
  cal,
  members,
  onClose,
  onDone,
}: {
  cal: Calendar;
  members: Appointment[];
  onClose: () => void;
  onDone: () => void;
}) {
  const queryClient = useQueryClient();
  const staff = useQuery(activeStaffQuery);
  const me = useQuery(meQuery).data;
  // UX only: the server checks package.sell (AC9.4).
  const maySell = can(me ?? null, "package.sell");
  const carts = useQuery({
    queryKey: ["group-carts", ...members.map((m) => m.id)],
    queryFn: () =>
      Promise.all(members.map((a) => cartFromAppointment(queryClient, a))),
    staleTime: 0,
  });
  const [rows, setRows] = useState<Record<string, Row>>(() =>
    Object.fromEntries(
      members.map((a) => [
        a.id,
        {
          staffId: a.staff_id ?? cal.technicians[0]?.id ?? "",
          mode: "cash",
          key: crypto.randomUUID(),
          state: "ready" as const,
        },
      ]),
    ),
  );
  const [busy, setBusy] = useState(false);
  const today = spaToday();
  const set = (id: string, patch: Partial<Row>) => {
    setRows((r) => {
      const row = r[id];
      return row ? { ...r, [id]: { ...row, ...patch } } : r;
    });
  };

  const linesOf = (i: number): CartLine[] | undefined => carts.data?.[i];
  const todo = members.filter((a, i) => {
    const row = rows[a.id];
    return (
      row !== undefined &&
      row.state !== "done" &&
      skipReason(a, linesOf(i)) === null &&
      row.staffId !== ""
    );
  });
  const allDone = members.every((a, i) => {
    const row = rows[a.id];
    return row?.state === "done" || skipReason(a, linesOf(i)) !== null;
  });

  const recordAll = async () => {
    setBusy(true);
    for (const a of todo) {
      const i = members.indexOf(a);
      const row = rows[a.id];
      const lines = linesOf(i);
      if (!row || !lines || !a.customer_id) continue;
      set(a.id, { state: "saving", error: undefined });
      const withStaff = lines.map((l) => ({ ...l, staffId: row.staffId }));
      const { sales, covered, due } = dueOf(withStaff, row.sale);
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { customer_id, ...visit } = visitBody(
        a.customer_id,
        withStaff,
        paymentsFor(due, row.mode, 0),
        "",
        {
          ...(a.business_date !== today
            ? { checkInAt: a.arrived_at ?? a.starts_at }
            : {}),
          ...(sales.length
            ? { packageSales: packageSalesBody(sales), covered }
            : {}),
        },
      );
      try {
        await completeAppointment(a.id, visit, row.key);
        set(a.id, { state: "done" });
        void queryClient.invalidateQueries({
          queryKey: ["customer-packages", a.customer_id],
        });
      } catch (e) {
        set(a.id, { state: "error", error: messageFor(e) });
      }
    }
    setBusy(false);
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["appointments"] }),
      queryClient.invalidateQueries({ queryKey: ["visits"] }),
      invalidateManagementViews(queryClient),
    ]);
  };

  const field = "min-h-11 rounded-[6px] border border-border bg-surface px-3";
  return (
    <BottomSheet
      open
      wide
      onClose={onClose}
      title={`Khách đến cả nhóm · ${String(members.length)} người`}
      description="Mỗi người là một lượt và một lần thu tiền riêng."
      footer={
        allDone ? (
          <button type="button" className="btn btn-primary" onClick={onDone}>
            Xong
          </button>
        ) : (
          <>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={onClose}
            >
              Thôi
            </button>
            <button
              type="button"
              className="btn btn-primary"
              disabled={busy || carts.isPending || todo.length === 0}
              onClick={() => {
                void recordAll();
              }}
            >
              {busy ? "Đang ghi…" : `Ghi ${String(todo.length)} lượt`}
            </button>
          </>
        )
      }
    >
      {carts.isPending && (
        <p role="status" className="text-text-muted">
          Đang chuẩn bị giỏ của từng người…
        </p>
      )}
      {carts.isError && (
        <p role="alert" className="text-danger">
          {messageFor(carts.error)}
        </p>
      )}
      <ul className="grid gap-3" aria-label="Người trong nhóm">
        {members.map((a, i) => {
          const row = rows[a.id];
          const lines = linesOf(i);
          const skip = skipReason(a, lines);
          const due = lines ? dueOf(lines, row?.sale).due : 0;
          if (!row) return null;
          return (
            <li
              key={a.id}
              aria-label={a.customer_name}
              className="grid gap-2 rounded-[10px] border border-border p-3"
            >
              <p className="flex flex-wrap items-baseline justify-between gap-2">
                <strong>
                  <CustomerName
                    name={a.customer_name}
                    gender={a.customer_gender}
                  />
                </strong>
                <span className="text-sm text-text-muted tabular-nums">
                  {hhmm(spaMinutes(a.starts_at))} · {a.duration_min} phút
                </span>
              </p>
              {lines && (
                <ul className="text-sm">
                  {lines.map((l) => (
                    <li key={l.key}>
                      {l.serviceName} ·{" "}
                      {l.source === "package"
                        ? `Trừ liệu trình (${l.packageName})`
                        : formatVnd(l.price)}
                    </li>
                  ))}
                </ul>
              )}
              {row.state === "done" ? (
                <p role="status" className="text-sm font-semibold text-success">
                  Đã ghi lượt{due > 0 ? ` · đã thu ${formatVnd(due)}` : ""}
                </p>
              ) : skip ? (
                <p className="text-sm text-warning-text">{skip}</p>
              ) : (
                <div className="flex flex-wrap items-end gap-2">
                  <label className="grid gap-1 text-sm">
                    <span>KTV làm</span>
                    <select
                      className={field}
                      value={row.staffId}
                      onChange={(e) => {
                        set(a.id, { staffId: e.target.value });
                      }}
                    >
                      <option value="">Chọn KTV…</option>
                      {(staff.data ?? []).map((s) => (
                        <option key={s.id} value={s.id}>
                          {s.full_name}
                        </option>
                      ))}
                    </select>
                  </label>
                  {due > 0 ? (
                    <div
                      role="group"
                      aria-label={`Thu tiền ${a.customer_name}`}
                      className="segmented"
                    >
                      {(
                        [
                          ["cash", "Tiền mặt"],
                          ["bank_transfer", "Chuyển khoản"],
                        ] as const
                      ).map(([m, label]) => (
                        <button
                          key={m}
                          type="button"
                          aria-pressed={row.mode === m}
                          onClick={() => {
                            set(a.id, { mode: m });
                          }}
                        >
                          {label}
                        </button>
                      ))}
                    </div>
                  ) : null}
                  <span className="text-sm font-semibold tabular-nums">
                    {due > 0 ? `Thu ${formatVnd(due)}` : "Không phát sinh tiền"}
                  </span>
                </div>
              )}
              {maySell && lines && !skip && row.state !== "done" && (
                <GroupPackagePick
                  person={a.customer_name}
                  sale={row.sale ?? null}
                  lines={lines}
                  staffId={row.staffId}
                  staff={staff.data ?? []}
                  onChange={(sale) => {
                    set(a.id, { sale });
                  }}
                />
              )}
              {row.state === "error" && row.error && (
                <p role="alert" className="text-sm text-danger">
                  {row.error}
                </p>
              )}
            </li>
          );
        })}
      </ul>
    </BottomSheet>
  );
}
