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
  amountDue,
  paymentsFor,
  visitBody,
} from "../front-desk/api";
import { cartFromAppointment } from "../front-desk/prefill";
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
      const due = amountDue(withStaff);
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { customer_id, ...visit } = visitBody(
        a.customer_id,
        withStaff,
        paymentsFor(due, row.mode, 0),
        "",
        a.business_date !== today
          ? { checkInAt: a.arrived_at ?? a.starts_at }
          : {},
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
              aria-busy={busy}
              aria-disabled={carts.isPending || todo.length === 0 || undefined}
              onClick={() => {
                if (!busy && !carts.isPending && todo.length > 0)
                  void recordAll();
              }}
            >
              {`Ghi ${String(todo.length)} lượt`}
            </button>
          </>
        )
      }
    >
      {carts.isPending && (
        <div aria-hidden="true" className="cal-skel-rows">
          {members.map((a) => (
            <div key={a.id} className="skeleton cal-skel-line" />
          ))}
        </div>
      )}
      {carts.isError && (
        <div role="alert" className="cal-error">
          <span>
            Không chuẩn bị được giỏ của từng người. {messageFor(carts.error)}
          </span>
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => void carts.refetch()}
          >
            Thử lại
          </button>
        </div>
      )}
      <ul className="appt-checkin" aria-label="Người trong nhóm">
        {members.map((a, i) => {
          const row = rows[a.id];
          const lines = linesOf(i);
          const skip = skipReason(a, lines);
          const due = lines ? amountDue(lines) : 0;
          if (!row) return null;
          const staffField = `checkin-staff-${a.id}`;
          return (
            <li key={a.id} aria-label={a.customer_name}>
              <p className="appt-checkin-head">
                <strong>
                  <CustomerName
                    name={a.customer_name}
                    gender={a.customer_gender}
                  />
                </strong>
                <span className="appt-hint tabular-nums">
                  {hhmm(spaMinutes(a.starts_at))} · {a.duration_min} phút
                </span>
              </p>
              {lines && (
                <ul className="appt-checkin-lines">
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
                <span role="status" className="badge badge-ok">
                  Đã ghi lượt{due > 0 ? ` · đã thu ${formatVnd(due)}` : ""}
                </span>
              ) : skip ? (
                <p className="appt-warn">{skip}</p>
              ) : (
                <div className="appt-actions">
                  <div className="field">
                    <label htmlFor={staffField}>KTV làm</label>
                    <select
                      id={staffField}
                      className="appt-input"
                      value={row.staffId}
                      aria-invalid={row.staffId === "" || undefined}
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
                  </div>
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
                  <span className="appt-due">
                    {due > 0 ? `Thu ${formatVnd(due)}` : "Không phát sinh tiền"}
                  </span>
                </div>
              )}
              {row.state !== "done" && !skip && row.staffId === "" && (
                <p className="appt-hint">Chọn KTV làm để ghi lượt người này.</p>
              )}
              {row.state === "error" && row.error && (
                <p role="alert" className="appt-error">
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
