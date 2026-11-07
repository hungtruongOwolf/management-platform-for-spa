import type { Appointment, Calendar } from "@vpyd/api-client";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import { Suspense, lazy, useState } from "react";
import { CustomerName } from "../../app/CustomerName";
import { BottomSheet } from "../../components/BottomSheet";
import { messageFor, retryable } from "../../lib/errors";
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
import { hhmm, spaMinutes, updateAppointment } from "./api";
import { completeAppointment } from "./complete";

const CustomerForm = lazy(() =>
  import("../customers/CustomerForm").then((m) => ({ default: m.CustomerForm })),
);

/**
 * A companion known only by a name (ADR-0009) gets a profile right here: the profile form with
 * the name filled in and the phone optional (decision E, ADR-0010), then the booking is linked to
 * it, so the person joins "Ghi N lượt" without leaving the group sheet.
 */
function GuestProfile({ a, onLinked }: { a: Appointment; onLinked: (linked: Appointment) => void }) {
  const me = useQuery(meQuery).data ?? null;
  const [creating, setCreating] = useState(false);
  const link = useMutation({
    mutationFn: (customerId: string) => updateAppointment(a, { customer_id: customerId }),
    onSuccess: (linked) => {
      setCreating(false);
      onLinked(linked);
    },
  });
  if (!can(me, "customer.write")) return null;
  return (
    <div className="grid gap-2">
      {!creating && (
        <button
          type="button"
          className="btn btn-secondary"
          onClick={() => {
            setCreating(true);
          }}
        >
          Tạo hồ sơ
        </button>
      )}
      {creating && (
        <Suspense fallback={<div aria-hidden="true" className="skeleton cal-skel-line" />}>
          <CustomerForm
            prefill={{
              full_name: a.customer_name,
              phone: (a.customer_phone ?? "").replace(/^\+84/, "0"),
            }}
            canWriteHealth={can(me, "customer.health_notes.write")}
            onSaved={(c) => {
              link.mutate(c.id);
            }}
            onOpenExisting={(id) => {
              link.mutate(id);
            }}
            onCancel={() => {
              setCreating(false);
            }}
          />
        </Suspense>
      )}
      {link.isError && (
        <p role="alert" className="field-error">
          {messageFor(link.error)}
        </p>
      )}
    </div>
  );
}

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
  if (a.is_guest) return "Chưa có hồ sơ: tạo hồ sơ (SĐT không bắt buộc) để ghi lượt cùng nhóm.";
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
  members: opened,
  onClose,
  onDone,
}: {
  cal: Calendar;
  members: Appointment[];
  onClose: () => void;
  onDone: () => void;
}) {
  const queryClient = useQueryClient();
  // The group as opened, with each companion linked to a new profile here replaced in place.
  const [members, setMembers] = useState(opened);
  const staff = useQuery(activeStaffQuery);
  const me = useQuery(meQuery).data;
  // UX only: the server checks package.sell (AC9.4).
  const maySell = can(me ?? null, "package.sell");
  const carts = useQuery({
    queryKey: ["group-carts", ...members.map((m) => `${m.id}:${m.customer_id ?? ""}`)],
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
        <div role="alert" className="alert">
          <span>
            Không chuẩn bị được giỏ của từng người. {messageFor(carts.error)}
          </span>
          {retryable(carts.error) && (
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => void carts.refetch()}
            >
              Thử lại
            </button>
          )}
        </div>
      )}
      <ul className="appt-checkin" aria-label="Người trong nhóm">
        {members.map((a, i) => {
          const row = rows[a.id];
          const lines = linesOf(i);
          const skip = skipReason(a, lines);
          const due = lines ? dueOf(lines, row?.sale).due : 0;
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
                <span className="field-hint tabular-nums">
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
                <div className="callout">
                  <p>{skip}</p>
                  {a.is_guest && (
                    <GuestProfile
                      a={a}
                      onLinked={(linked) => {
                        setMembers((ms) => ms.map((m) => (m.id === linked.id ? linked : m)));
                        void queryClient.invalidateQueries({ queryKey: ["appointments"] });
                      }}

                    />
                  )}
                </div>
              ) : (

                <div className="button-row">
                  <div className="field">
                    <label htmlFor={staffField}>KTV làm</label>
                    <select
                      id={staffField}
                      className="input"
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
                <p className="field-hint">Chọn KTV làm để ghi lượt người này.</p>
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
                <p role="alert" className="field-error">
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
