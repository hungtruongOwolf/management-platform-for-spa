import type {
  Appointment,
  Calendar,
  Customer,
  TimelinePhase,
} from "@vpyd/api-client";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { formatVnd } from "@vpyd/ui";
import { useRef, useState } from "react";
import { CustomerName } from "../../app/CustomerName";
import { Icon } from "../../app/Icon";
import { BottomSheet } from "../../components/BottomSheet";
import { ApiError } from "../../lib/api";
import { messageFor } from "../../lib/errors";
import { useReached } from "../../lib/useReached";
import { spaToday } from "../corrections/api";
import { createCustomer, createWalkIn, customerQuery } from "../customers/api";
import { activeStaffQuery, formatDate } from "../sales/api";
import { putAppointment } from "./cache";
import { range, servicesText } from "./TimeGrid";
import {
  PHASE_BADGE,
  PHASE_LABEL,
  arriveAppointment,
  closeAppointment,
  isOpen,
  paymentLabel,
  spaMinutes,
  undoArrival,
  updateAppointment,
} from "./api";

const WEEKDAY = ["CN", "Thứ 2", "Thứ 3", "Thứ 4", "Thứ 5", "Thứ 6", "Thứ 7"];

/** "2026-11-06" → "Thứ 6, 06/11". */
export function dayTitle(day: string): string {
  const d = new Date(`${day}T00:00:00Z`);
  return `${WEEKDAY[d.getUTCDay()] ?? ""}, ${formatDate(day).slice(0, 5)}`;
}

/** The phase as the app's one status chip set (always with its words). */
export function PhaseBadge({ phase }: { phase: TimelinePhase }) {
  return (
    <span className={`badge ${PHASE_BADGE[phase]}`}>{PHASE_LABEL[phase]}</span>
  );
}

/** "KTV Lan", without doubling a name that already says "KTV …"; "Chưa gán KTV" when nobody. */
export function staffText(name: string | null | undefined): string {
  if (name === null || name === undefined) return "Chưa gán KTV";
  return /^ktv\b/i.test(name) ? name : `KTV ${name}`;
}

/** "0905 …" for the +84 form the server keeps; null stays null. */
function localPhone(phone: string | null | undefined): string | null {
  return phone ? phone.replace(/^\+84/, "0") : null;
}

/** "KTV Lan đã có lịch 10:00–11:30 (Nguyễn Thị Hà)" for a 409 staff_busy; else the usual text. */
export function conflictMessage(error: unknown, cal: Calendar): string {
  if (error instanceof ApiError) {
    if (error.code === "appointment.staff_busy" && error.existingId) {
      const other = cal.appointments.find((x) => x.id === error.existingId);
      if (other)
        return `${other.staff_name ?? "KTV"} đã có lịch ${range(spaMinutes(other.starts_at), other.duration_min)} (${other.customer_name}). Chọn KTV khác.`;
    }
    if (
      error.code === "validation_failed" &&
      error.errors.some(
        (f) => f.field === "customer_id" && f.code === "duplicate",
      )
    )
      return "Hồ sơ này đã là một người khác trong nhóm.";
  }
  return messageFor(error);
}

type Kind = "arrive" | "undo" | "close" | "profile";

/** One request of the sheet, tagged with the control that sent it. */
interface Job {
  kind: Kind;
  run: () => Promise<Appointment>;
}

/**
 * One booking (spec appointments.md §10.12, EM 2026-11-07 "đặt lịch là 2 người rồi dừng ở
 * đó"): every person of a group is served from their own sheet, like a single booking. Order:
 * what is booked → the group as links → who serves + arrival → the rare ways out. The technician
 * is a plain update saved on change; a companion without a profile gets one in one tap.
 */
export function AppointmentSheet({
  a,
  cal,
  locked,
  canCreateCustomer,
  onClose,
  onEdit,
  onComplete,
  onOpenOther,
}: {
  a: Appointment;
  cal: Calendar;
  locked: boolean;
  canCreateCustomer: boolean;
  onClose: () => void;
  onEdit: () => void;
  onComplete: () => void;
  onOpenOther: (id: string) => void;
}) {
  const queryClient = useQueryClient();
  const staff = useQuery(activeStaffQuery);
  // The booked technician, or nobody: never a guess (EM 2026-11-07).
  const [staffId, setStaffId] = useState(a.staff_id ?? "");
  // The newest copy this sheet saved: a second change must not send a stale version.
  const latest = useRef<Appointment>(a);
  if (a.version > latest.current.version) latest.current = a;
  const [arriveTried, setArriveTried] = useState(false);
  const [profileTried, setProfileTried] = useState(false);
  const [closing, setClosing] = useState<"cancel" | "no-show" | null>(null);
  // "Hoàn tác đã đến" asks once before it sends (EM 2026-11-07 item 5a).
  const [undoAsked, setUndoAsked] = useState(false);
  const [reason, setReason] = useState("");
  const [reasonTried, setReasonTried] = useState(false);
  // A newcomer's phone already on a profile that is not in this group: use it, or go without.
  const [taken, setTaken] = useState<Customer | null>(null);
  const refresh = () =>
    queryClient.invalidateQueries({ queryKey: ["appointments"] });
  const action = useMutation({
    mutationFn: (job: Job) => job.run(),
    onSuccess: (saved) => {
      latest.current = saved;
      setClosing(null);
      setTaken(null);
      setUndoAsked(false);
      // The answer shows at once (appointments/cache.ts); the range refetches behind it.
      putAppointment(queryClient, saved);
      void refresh();
    },
  });
  /** Which button sent the running request (its spinner; the others wait). */
  const running: Kind | null = action.isPending ? action.variables.kind : null;
  const send = (kind: Kind, run: () => Promise<Appointment>) => {
    if (action.isPending) return;
    action.mutate({ kind, run });
  };
  /** The refusal of the last request, shown by the control that sent it. */
  const errorFor = (kind: Kind): string | null =>
    action.isError && action.variables.kind === kind
      ? conflictMessage(action.error, cal)
      : null;
  // The technician is its own plain update (PATCH staff_id), saved on change; arrival may run
  // alongside it (the arrival names the technician itself).
  const staffSave = useMutation({
    mutationFn: (next: string) =>
      updateAppointment(latest.current, {
        staff_id: next === "" ? null : next,
      }),
    onSuccess: (saved) => {
      latest.current = saved;
      // The calendar (and so the cart a press on the primary action fills) knows it first.
      putAppointment(queryClient, saved);
      void refresh();
    },
    onError: () => {
      // A refused technician goes back to the saved one; the message says why.
      setStaffId(latest.current.staff_id ?? "");
    },
  });

  const today = spaToday();
  const open = isOpen(a);
  const editable = open && !locked;
  // "Không đến" appears once the time passes, also while the sheet stays open (ADR-0019 D20).
  const started = useReached(new Date(a.starts_at).getTime());
  const start = spaMinutes(a.starts_at);
  const group = a.group_id
    ? cal.appointments.filter((x) => x.group_id === a.group_id)
    : [];
  const canServe = editable && a.business_date <= today;
  const canRecord = canServe && !a.is_guest;
  const canArrive =
    editable && a.status === "booked" && a.business_date === today;
  // An arrival marked by mistake goes back to booked until the visit is recorded (server rule).
  const canUndoArrival =
    editable && a.status === "arrived" && a.business_date === today;
  const ids = {
    staff: `appt-staff-${a.id}`,
    staffMsg: `appt-staff-${a.id}-msg`,
    profile: `appt-profile-${a.id}`,
    close: `appt-close-${a.id}`,
    reason: `appt-reason-${a.id}`,
  };
  const reasonMissing = reason.trim() === "";
  const reasonLabel =
    closing === "cancel" ? "Lý do hủy lịch" : "Ghi chú không đến";

  const link = (customerId: string) =>
    updateAppointment(latest.current, { customer_id: customerId });
  /** "Tạo hồ sơ": the booked name (and phone), nothing else to type (ADR-0011: phone optional). */
  const createProfile = (withPhone: boolean) => {
    send("profile", async () => {
      try {
        const c = await createCustomer({
          full_name: a.customer_name,
          phone: withPhone ? localPhone(a.customer_phone) : null,
        });
        return await link(c.id);
      } catch (e) {
        if (
          withPhone &&
          e instanceof ApiError &&
          e.code === "customer.phone_taken" &&
          e.existingId
        ) {
          // One family, one phone (spec Q11): a member already has it, so this one goes without.
          if (group.some((x) => x.customer_id === e.existingId))
            return link(
              (
                await createCustomer({
                  full_name: a.customer_name,
                  phone: null,
                })
              ).id,
            );
          setTaken(await queryClient.query(customerQuery(e.existingId)));
        }
        throw e;
      }
    });
  };
  const asWalkIn = () => {
    send("profile", async () => link((await createWalkIn()).id));
  };

  const saveStaff = (next: string) => {
    // One change at a time: the select shows what is saved or being saved, never a lost pick.
    if (staffSave.isPending) return;
    setStaffId(next);
    if (next !== (latest.current.staff_id ?? "")) staffSave.mutate(next);
  };

  const profileError =
    action.error instanceof ApiError &&
    action.error.code === "customer.phone_taken"
      ? null
      : errorFor("profile");
  const staffError = staffSave.isError
    ? conflictMessage(staffSave.error, cal)
    : errorFor("arrive");
  const staffMessage: { error: boolean; text: string } | null = staffError
    ? { error: true, text: staffError }
    : staffSave.isPending
      ? { error: false, text: "Đang lưu KTV…" }
      : arriveTried && staffId === ""
        ? { error: true, text: "Chọn KTV làm rồi bấm lại “Chỉ báo đã đến”." }
        : staffSave.isSuccess
          ? { error: false, text: "Đã lưu KTV." }
          : staffId === ""
            ? {
                error: false,
                text: "Chưa gán KTV: chọn khi biết ai làm, đổi được tới lúc ghi lượt.",
              }
            : null;

  return (
    <BottomSheet
      open
      onClose={onClose}
      title={`${range(start, a.duration_min)} · ${a.customer_name}`}
      description={
        <>
          {dayTitle(a.business_date)} · <PhaseBadge phase={a.phase} />
        </>
      }
      footer={
        editable ? (
          <>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={onEdit}
            >
              Đổi lịch
            </button>
            {a.business_date <= today && (
              <button
                type="button"
                className="btn btn-primary"
                // Focusable while it waits: a press says what is missing, at its control.
                aria-disabled={!canRecord || undefined}
                aria-describedby={a.is_guest ? ids.profile : undefined}
                onClick={() => {
                  if (canRecord) {
                    onComplete();
                    return;
                  }
                  setProfileTried(true);
                  document
                    .querySelector<HTMLElement>(`#${ids.profile} button`)
                    ?.focus();
                }}
              >
                {a.status === "arrived" ? "Ghi lượt" : "Khách đến · ghi lượt"}
              </button>
            )}
          </>
        ) : (
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            Đóng
          </button>
        )
      }
    >
      <div className="appt-sheet">
        <dl className="detail-grid">
          <dt>Khách</dt>
          <dd>
            <CustomerName name={a.customer_name} gender={a.customer_gender} />
            {a.customer_id && (
              <>
                {" · "}
                <Link
                  to="/khach-hang/$customerId"
                  params={{ customerId: a.customer_id }}
                  className="link whitespace-nowrap underline"
                >
                  Mở hồ sơ
                </Link>
              </>
            )}
          </dd>
          <dt>SĐT</dt>
          <dd className="tabular-nums">
            {localPhone(a.customer_phone) ?? "Chưa có SĐT"}
          </dd>
          <dt>Dịch vụ</dt>
          <dd>
            {servicesText(a.services)} ({a.duration_min} phút)
          </dd>
          {!editable && (
            <>
              <dt>KTV</dt>
              <dd>{a.staff_name ?? "Chưa gán"}</dd>
            </>
          )}
          <dt>Thanh toán</dt>
          <dd>{paymentLabel(a, formatVnd)}</dd>
          {a.companions.length > 0 && (
            <>
              <dt>Đi cùng</dt>
              <dd>{a.companions.join(", ")}</dd>
            </>
          )}
          {a.note && (
            <>
              <dt>Ghi chú</dt>
              <dd>{a.note}</dd>
            </>
          )}
          {a.status_reason && (
            <>
              <dt>Lý do</dt>
              <dd>{a.status_reason}</dd>
            </>
          )}
        </dl>

        {editable && a.is_guest && (
          // One line, one tap (EM 2026-11-07: the big banner blocked the check-in).
          <div
            id={ids.profile}
            role="group"
            aria-label="Hồ sơ khách"
            className="appt-profile"
          >
            <p className="appt-profile-text">
              <strong>Chưa có hồ sơ khách.</strong>{" "}
              {canCreateCustomer
                ? "Ghi lượt cần hồ sơ: tạo từ tên đã đặt, hoặc ghi là khách vãng lai."
                : "Ghi lượt cần hồ sơ: nhờ người có quyền tạo hồ sơ khách."}
            </p>
            {canCreateCustomer && !taken && (
              <div className="button-row">
                <button
                  type="button"
                  className="btn btn-secondary"
                  aria-busy={running === "profile" || undefined}
                  onClick={() => {
                    createProfile(a.customer_phone != null);
                  }}
                >
                  Tạo hồ sơ
                </button>
                <button
                  type="button"
                  className="btn btn-ghost"
                  onClick={asWalkIn}
                >
                  Ghi là khách vãng lai
                </button>
              </div>
            )}
            {taken && (
              <div role="alert" className="appt-profile-taken">
                <p>
                  SĐT {localPhone(a.customer_phone)} đã có hồ sơ{" "}
                  <strong>{taken.full_name}</strong>.
                </p>
                <div className="button-row">
                  <button
                    type="button"
                    className="btn btn-secondary"
                    onClick={() => {
                      send("profile", () => link(taken.id));
                    }}
                  >
                    Dùng hồ sơ này
                  </button>
                  <button
                    type="button"
                    className="btn btn-ghost"
                    onClick={() => {
                      setTaken(null);
                      createProfile(false);
                    }}
                  >
                    Tạo hồ sơ không SĐT
                  </button>
                </div>
              </div>
            )}
            {/* The phone already taken is asked above; any other refusal is said here. */}
            {profileError && (
              <p role="alert" className="field-error">
                {profileError}
              </p>
            )}
            {profileTried && !errorFor("profile") && !taken && (
              <p role="alert" className="field-error">
                Chọn “Tạo hồ sơ” hoặc “Ghi là khách vãng lai” rồi bấm lại.
              </p>
            )}
          </div>
        )}

        {group.length > 1 && (
          <section className="appt-section" aria-labelledby={`group-${a.id}`}>
            <p id={`group-${a.id}`} className="section-label">
              Cùng nhóm · {group.length} người
            </p>
            {/* Each person is their own booking: a row opens their sheet (arrive, KTV, record). */}
            <ul className="appt-members">
              {group.map((x) => {
                const inner = (
                  <>
                    <span className="appt-member-name">{x.customer_name}</span>
                    <PhaseBadge phase={x.phase} />
                    <span className="appt-member-meta">
                      {range(spaMinutes(x.starts_at), x.duration_min)} ·{" "}
                      {servicesText(x.services)} · {staffText(x.staff_name)}
                    </span>
                  </>
                );
                return (
                  <li key={x.id}>
                    {x.id === a.id ? (
                      <div className="appt-member" aria-current="true">
                        {inner}
                      </div>
                    ) : (
                      <button
                        type="button"
                        className="appt-member"
                        onClick={() => {
                          onOpenOther(x.id);
                        }}
                      >
                        {inner}
                        <Icon name="chevron-right" />
                      </button>
                    )}
                  </li>
                );
              })}
            </ul>
          </section>
        )}

        {locked && open && (
          <p className="note">Ngày đã chốt, liên hệ chủ spa.</p>
        )}

        {editable && (
          <section className="appt-section" aria-label="KTV và khách đến">
            <div className="button-row">
              <div className="field">
                <label htmlFor={ids.staff}>KTV làm</label>
                <select
                  id={ids.staff}
                  className="input"
                  value={staffId}
                  aria-invalid={
                    (arriveTried && staffId === "") ||
                    staffError !== null ||
                    undefined
                  }
                  aria-describedby={ids.staffMsg}
                  aria-busy={staffSave.isPending || undefined}
                  onChange={(e) => {
                    saveStaff(e.target.value);
                  }}
                >
                  {/* An arrived customer is being served by someone (server rule). */}
                  {a.status === "booked" && (
                    <option value="">Chưa gán KTV</option>
                  )}
                  {(staff.data ?? []).map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.full_name}
                    </option>
                  ))}
                </select>
              </div>
              {canArrive && (
                <button
                  type="button"
                  className="btn btn-secondary"
                  aria-busy={running === "arrive" || undefined}
                  onClick={() => {
                    setArriveTried(true);
                    if (staffId !== "")
                      send("arrive", () => arriveAppointment(a.id, staffId));
                    else document.getElementById(ids.staff)?.focus();
                  }}
                >
                  Chỉ báo đã đến
                </button>
              )}
            </div>
            <StaffNote id={ids.staffMsg} message={staffMessage} />
            {canUndoArrival &&
              (undoAsked ? (
                <div role="group" aria-labelledby={`undo-${a.id}`} className="grid gap-2">
                  <p id={`undo-${a.id}`}>
                    Đưa lịch về <strong>Chưa đến</strong>? Giờ đến được xóa, KTV làm giữ nguyên.
                  </p>
                  <div className="button-row">
                    <button
                      type="button"
                      className="btn btn-primary"
                      aria-busy={running === "undo" || undefined}
                      onClick={() => {
                        send("undo", () => undoArrival(a.id));
                      }}
                    >
                      Hoàn tác đã đến
                    </button>
                    <button
                      type="button"
                      className="btn btn-secondary"
                      onClick={() => {
                        setUndoAsked(false);
                      }}
                    >
                      Thôi
                    </button>
                  </div>
                  {errorFor("undo") && (
                    <p role="alert" className="field-error">
                      {errorFor("undo")}
                    </p>
                  )}
                </div>
              ) : (
                <div className="button-row">
                  <button
                    type="button"
                    className="btn btn-ghost"
                    onClick={() => {
                      setUndoAsked(true);
                    }}
                  >
                    <Icon name="history" /> Hoàn tác đã đến
                  </button>
                </div>
              ))}
          </section>
        )}

        {editable && (
          <section
            className="appt-section appt-close"
            aria-labelledby={ids.close}
          >
            <p id={ids.close} className="sr-only">
              Khách không đến hoặc hủy lịch
            </p>
            {closing === null ? (
              <div className="appt-close-actions">
                {a.status === "booked" && started && (
                  <button
                    type="button"
                    className="btn btn-secondary"
                    onClick={() => {
                      setClosing("no-show");
                      setReason("");
                      setReasonTried(false);
                    }}
                  >
                    Không đến
                  </button>
                )}
                <button
                  type="button"
                  className="btn btn-danger-outline"
                  onClick={() => {
                    setClosing("cancel");
                    setReason("");
                    setReasonTried(false);
                  }}
                >
                  <Icon name="x" /> Hủy lịch
                </button>
              </div>
            ) : (
              <form
                className="appt-form"
                noValidate
                onSubmit={(e) => {
                  e.preventDefault();
                  setReasonTried(true);
                  if (reasonMissing) return;
                  const outcome = closing;
                  send("close", () =>
                    closeAppointment(a.id, outcome, reason.trim()),
                  );
                }}
              >
                <div className="field">
                  <label htmlFor={ids.reason}>{reasonLabel} (bắt buộc)</label>
                  <input
                    id={ids.reason}
                    autoFocus
                    value={reason}
                    maxLength={500}
                    onChange={(e) => {
                      setReason(e.target.value);
                    }}
                    className="input"
                    aria-invalid={reasonTried && reasonMissing}
                    aria-describedby={`${ids.reason}-msg`}
                  />
                  {reasonTried && reasonMissing ? (
                    <span id={`${ids.reason}-msg`} className="field-error">
                      Nhập {reasonLabel.toLowerCase()} rồi bấm lại.
                    </span>
                  ) : (
                    <span id={`${ids.reason}-msg`} className="field-hint">
                      Ghi vào lịch hẹn, ai mở lịch cũng thấy.
                    </span>
                  )}
                </div>
                <div className="button-row">
                  <button
                    type="submit"
                    className={
                      closing === "cancel"
                        ? "btn btn-danger"
                        : "btn btn-primary"
                    }
                    aria-busy={running === "close" || undefined}
                  >
                    {closing === "cancel" ? "Hủy lịch" : "Đánh dấu không đến"}
                  </button>
                  <button
                    type="button"
                    className="btn btn-secondary"
                    onClick={() => {
                      setClosing(null);
                    }}
                  >
                    Thôi
                  </button>
                </div>
                {errorFor("close") && (
                  <p role="alert" className="field-error">
                    {errorFor("close")}
                  </p>
                )}
              </form>
            )}
          </section>
        )}
      </div>
    </BottomSheet>
  );
}

/** The one line under "KTV làm": saving, saved, why it waits, or the refusal. */
function StaffNote({
  id,
  message,
}: {
  id: string;
  message: { error: boolean; text: string } | null;
}) {
  if (!message) return <span id={id} className="sr-only" />;
  return message.error ? (
    <span id={id} role="alert" className="field-error">
      {message.text}
    </span>
  ) : (
    <span id={id} role="status" className="field-hint">
      {message.text}
    </span>
  );
}
