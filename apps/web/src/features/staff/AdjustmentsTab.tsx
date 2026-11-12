import { useMutation, useQuery, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Icon } from "../../app/Icon";
import { BottomSheet } from "../../components/BottomSheet";
import { DateField } from "../../components/DateField";
import { ResponsiveTable, type Column } from "../../components/ResponsiveTable";
import { RowMenu } from "../../components/RowMenu";
import { fieldMessage, messageFor } from "../../lib/errors";
import { meQuery } from "../auth/session";
import { LazyRecordTrace, usePreloadRecordHistory } from "../record-history/lazy";
import { canTrace } from "../record-history/query";
import { EmptyState, ErrorState, RowsSkeleton } from "../reports/ui";
import { activeStaffQuery } from "../sales/api";
import {
  adjustmentsQuery,
  createAdjustment,
  deleteAdjustment,
  invalidateStaff,
  reviewsQuery,
  updateAdjustment,
  type Adjustment,
  STAFF_FIELDS,
} from "./api";
import { dayLabel, vnd } from "./ui";

/** "Thưởng" / "Phạt" + amount and points, as the chips and the log show an entry. */
export function adjustmentText(a: Adjustment): string {
  return [
    a.kind === "bonus" ? "Thưởng" : "Phạt",
    a.amount != null ? vnd(a.amount) : "",
    a.points != null ? `${String(a.points)} điểm` : "",
  ]
    .filter(Boolean)
    .join(" ");
}

/** "Sửa" / "Xóa" of one entry, shown only when the server says the caller may (`can_edit`). */
export function AdjustmentActions({ entry, name, onEdit }: { entry: Adjustment; name: string; onEdit: () => void }) {
  const [confirming, setConfirming] = useState(false);
  return (
    <span className="inline-flex gap-1">
      <button type="button" className="btn btn-ghost" aria-label={`Sửa ${adjustmentText(entry)} của ${name}`} onClick={onEdit}>
        Sửa
      </button>
      <button
        type="button"
        className="btn btn-ghost text-danger"
        aria-label={`Xóa ${adjustmentText(entry)} của ${name}`}
        onClick={() => {
          setConfirming(true);
        }}
      >
        Xóa
      </button>
      {confirming && (
        <DeleteAdjustment
          entry={entry}
          name={name}
          onClose={() => {
            setConfirming(false);
          }}
        />
      )}
    </span>
  );
}

/** Confirm before removing an entry: it leaves the log and the payslip (the server keeps a trace). */
function DeleteAdjustment({ entry, name, onClose }: { entry: Adjustment; name: string; onClose: () => void }) {
  const queryClient = useQueryClient();
  const remove = useMutation({
    mutationFn: () => deleteAdjustment(entry.id),
    onSuccess: onClose,
    onSettled: () => invalidateStaff(queryClient),
  });
  return (
    <BottomSheet
      open
      onClose={onClose}
      title={`Xóa ${adjustmentText(entry).toLowerCase()} của ${name}?`}
      description={`${dayLabel(entry.occurred_on)} · ${entry.reason}`}
      footer={
        <>
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            Thôi
          </button>
          <button
            type="button"
            className="btn btn-danger"
            aria-busy={remove.isPending || undefined}
            onClick={() => {
              if (!remove.isPending) remove.mutate();
            }}
          >
            Xóa khoản này
          </button>
        </>
      }
    >
      <p className="text-sm">Khoản này sẽ không còn trong sổ thưởng/phạt và không tính vào lương tháng. Lịch sử thao tác vẫn giữ lại.</p>
      {remove.isError && (
        <p role="alert" className="text-sm text-danger">
          {messageFor(remove.error)}
        </p>
      )}
    </BottomSheet>
  );
}

/**
 * The month's bonus/penalty log (end of "Bảng công tháng"), newest first. An entry recorded by
 * mistake can be edited or removed while the month is open, by whoever may record it (decision C:
 * a manager never for themselves or another manager). Day-to-day entries come from the daily book.
 */
export function AdjustmentLogSection({ month, today, canWrite }: { month: string; today: string; canWrite: boolean }) {
  const log = useQuery(adjustmentsQuery(month));
  const staff = useQuery(activeStaffQuery);
  // Who the caller may record for (decision C), from the month's review sheet.
  const reviews = useQuery(reviewsQuery(month));
  const { data: me } = useSuspenseQuery(meQuery);
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<Adjustment | null>(null);
  // "Xem ai ghi, ai sửa" in the row's ⋯ (ADR-0022, design §2.9; UX only, the server decides).
  const traced = canTrace(me, "adjustment");
  usePreloadRecordHistory(traced);
  /** The entry whose history is open under its row. */
  const [tracing, setTracing] = useState<string | null>(null);

  if (log.isPending) return <RowsSkeleton rows={4} />;
  if (log.isError) return <ErrorState error={log.error} onRetry={() => void log.refetch()} />;
  // Someone who left this month is no longer in the active list, but is in the month's sheet.
  const names = new Map([
    ...(reviews.data?.staff ?? []).map((s) => [s.staff_id, s.full_name] as const),
    ...(staff.data ?? []).map((s) => [s.id, s.full_name] as const),
  ]);
  const columns: Column<Adjustment>[] = [
    { id: "who", header: "Nhân viên", card: "title", cell: (a) => names.get(a.staff_id) ?? "Nhân viên đã nghỉ" },
    { id: "day", header: "Ngày", card: "subtitle", cell: (a) => dayLabel(a.occurred_on) },
    {
      id: "kind",
      header: "Loại",
      card: "aside",
      cell: (a) => <span className={`badge ${a.kind === "bonus" ? "badge-ok" : "badge-danger"}`}>{a.kind === "bonus" ? "Thưởng" : "Phạt"}</span>,
    },
    {
      id: "amount",
      header: "Số tiền / điểm",
      numeric: true,
      cell: (a) =>
        [a.amount != null ? `${a.kind === "penalty" ? "−" : "+"}${vnd(a.amount)}` : "", a.points != null ? `${String(a.points)} điểm` : ""]
          .filter(Boolean)
          .join(" · "),
    },
    {
      id: "reason",
      header: "Lý do",
      cell: (a) => (
        <>
          {a.reason}
          {a.corrects_month && <span className="block text-xs text-text-muted">Điều chỉnh tháng {a.corrects_month}</span>}
        </>
      ),
    },
    { id: "by", header: "Người ghi", cell: (a) => a.by_name },
    {
      id: "actions",
      header: "Thao tác",
      card: "aside",
      cell: (a) =>
        canWrite && a.can_edit ? (
          <AdjustmentActions
            entry={a}
            name={names.get(a.staff_id) ?? "nhân viên"}
            onEdit={() => {
              setEditing(a);
            }}
          />
        ) : null,
    },
    ...(traced
      ? [
          {
            id: "more",
            header: <span className="sr-only">Thêm</span>,
            card: "actions",
            className: "text-right",
            cell: (a: Adjustment) => (
              <RowMenu
                label={`Thao tác ${adjustmentText(a)} của ${names.get(a.staff_id) ?? "nhân viên"}`}
                items={[
                  {
                    label: tracing === a.id ? "Ẩn ai ghi, ai sửa" : "Xem ai ghi, ai sửa",
                    onSelect: () => {
                      setTracing((shown) => (shown === a.id ? null : a.id));
                    },
                  },
                ]}
              />
            ),
          } satisfies Column<Adjustment>,
        ]
      : []),
  ];
  const recordable = (reviews.data?.staff ?? []).filter((s) => s.can_record).map((s) => ({ id: s.staff_id, full_name: s.full_name }));

  return (
    <section className="grid content-start gap-3" aria-labelledby="adjustment-log-title">
      <div className="flex flex-wrap items-center justify-between gap-3">
        {/* Programmatic focus target (tabIndex -1): where focus goes when a row's history hides on 403. */}
        <h2 id="adjustment-log-title" className="panel-title" tabIndex={-1}>
          Sổ thưởng, phạt của tháng
        </h2>
        {canWrite && (
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => {
              setOpen(true);
            }}
          >
            <Icon name="plus" className="size-[18px]" />
            Ghi thưởng / phạt
          </button>
        )}
      </div>
      {log.data.items.length === 0 ? (
        <div className="card">
          <EmptyState title="Chưa có khoản thưởng hay phạt nào trong tháng này">
            {canWrite ? "Bấm “Ghi thưởng / phạt” để ghi một khoản; khoản ghi trong Sổ hằng ngày cũng hiện ở đây." : "Khoản ghi trong Sổ hằng ngày sẽ hiện ở đây."}
          </EmptyState>
        </div>
      ) : (
        <div className="card overflow-hidden">
          <ResponsiveTable
            caption="Sổ thưởng phạt"
            rows={log.data.items}
            rowKey={(a) => a.id}
            columns={columns}
            expanded={(a) =>
              tracing === a.id ? (
                // Under its row, open from the start (asked for from the menu, whose item is gone).
                <div className="px-3 pb-3">
                  <LazyRecordTrace
                    record={{ type: "adjustment", id: a.id }}
                    noun={`${adjustmentText(a)} của ${names.get(a.staff_id) ?? "nhân viên"}`}
                    day={a.occurred_on}
                    defaultOpen
                    autoFocus
                    // The ⋯ column leaves with the right: the log's heading stays (K31).
                    focusOnHide={() => document.getElementById("adjustment-log-title")}
                  />
                </div>
              ) : null
            }
          />
        </div>
      )}
      {open && (
        <AdjustmentForm
          today={today}
          closedMonth={log.data.closed ? log.data.month : null}
          staff={recordable}
          onClose={() => {
            setOpen(false);
          }}
        />
      )}
      {editing && (
        <AdjustmentForm
          today={today}
          closedMonth={null}
          staff={recordable}
          fixed={{ staffId: editing.staff_id, name: names.get(editing.staff_id) ?? "nhân viên", date: editing.occurred_on }}
          editing={editing}
          onClose={() => {
            setEditing(null);
          }}
        />
      )}
    </section>
  );
}

/** Points per entry, as the server bounds them (hr/src/adjustment.rs `MAX_POINTS`). */
const MAX_POINTS = 100;

/** What a bonus / penalty form still lacks before it can be saved ("" when complete). */
export function adjustmentMissing(f: { staffId: string; amount: string; points: string; reason: string }): string {
  const missing = [
    f.staffId ? "" : "chọn nhân viên",
    f.amount || f.points ? "" : "nhập số tiền hoặc số điểm",
    f.reason.trim() ? "" : "ghi lý do",
  ].filter(Boolean);
  return missing.length ? `Để lưu, cần ${missing.join(", ")}.` : "";
}

/**
 * Bonus / penalty form. From the daily book it opens for one person and one day (`fixed`): no
 * person picker, the date pre-filled. With `editing` it corrects that entry (`PUT`: the old one
 * is voided, the corrected one recorded).
 */
export function AdjustmentForm({
  today,
  closedMonth,
  staff,
  fixed,
  editing,
  onClose,
}: {
  today: string;
  closedMonth: string | null;
  staff: { id: string; full_name: string }[];
  fixed?: { staffId: string; name: string; date: string };
  editing?: Adjustment;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const [key] = useState(() => crypto.randomUUID());
  const [staffId, setStaffId] = useState(fixed?.staffId ?? staff[0]?.id ?? "");
  const [kind, setKind] = useState<"bonus" | "penalty">(editing?.kind ?? "bonus");
  const [amount, setAmount] = useState(editing?.amount != null ? String(editing.amount) : "");
  const [points, setPoints] = useState(editing?.points != null ? String(editing.points) : "");
  const [reason, setReason] = useState(editing?.reason ?? "");
  const [on, setOn] = useState(fixed?.date ?? today);
  // The staff list may still be loading when the form opens without a fixed person.
  const chosen = staffId || (fixed ? "" : (staff[0]?.id ?? ""));
  const save = useMutation({
    mutationFn: () => {
      const body = {
        staff_id: chosen,
        kind,
        amount: amount ? Number(amount.replace(/\D/g, "")) : null,
        points: points ? Number(points) : null,
        reason: reason.trim(),
        occurred_on: on,
        // Viewing a closed month: the entry goes into today's month and names the closed one.
        corrects_month: editing ? (editing.corrects_month ?? null) : closedMonth,
      };
      return editing ? updateAdjustment(editing.id, body, key) : createAdjustment(body, key);
    },
    onSuccess: async () => {
      await invalidateStaff(queryClient);
      onClose();
    },
  });
  const missing = adjustmentMissing({ staffId: chosen, amount, points, reason });
  const err = (f: string) => fieldMessage(save.error, f, STAFF_FIELDS);
  const pointsOutOfRange = points !== "" && (Number(points) < 1 || Number(points) > MAX_POINTS);
  const pointsError = pointsOutOfRange || err("points") ? `Từ 1 đến ${String(MAX_POINTS)} điểm.` : undefined;
  const reasonError = err("reason") ? "Ghi lý do, tối đa 500 ký tự." : undefined;
  const formId = "adjustment-form";
  return (
    <BottomSheet
      open
      onClose={onClose}
      title={editing ? `Sửa thưởng / phạt · ${fixed?.name ?? ""}` : fixed ? `Thưởng / phạt · ${fixed.name}` : "Ghi thưởng / phạt"}
      description={closedMonth ? `Điều chỉnh cho tháng ${closedMonth} (đã chốt), ghi vào tháng hiện tại` : undefined}
      footer={
        <>
          {missing && (
            <span id="adjustment-missing" className="text-sm text-text-muted">
              {missing}
            </span>
          )}
          <button
            type="submit"
            form={formId}
            className="btn btn-primary"
            disabled={save.isPending || missing !== "" || pointsOutOfRange}
            aria-describedby={missing ? "adjustment-missing" : undefined}
          >
            Lưu
          </button>
        </>
      }
    >
      <form
        id={formId}
        className="grid gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (missing || pointsOutOfRange || save.isPending) return;
          save.mutate();
        }}
      >
        {!fixed && (
          <label className="filter-field">
            Nhân viên
            <select
              value={chosen}
              onChange={(e) => {
                setStaffId(e.target.value);
              }}
            >
              {staff.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.full_name}
                </option>
              ))}
            </select>
          </label>
        )}
        <div className="segmented" role="group" aria-label="Loại">
          {(
            [
              ["bonus", "Thưởng"],
              ["penalty", "Phạt"],
            ] as const
          ).map(([k, label]) => (
            <button
              key={k}
              type="button"
              aria-pressed={kind === k}
              onClick={() => {
                setKind(k);
              }}
            >
              {label}
            </button>
          ))}
        </div>
        <div className="grid grid-cols-2 gap-4">
          <label className="filter-field">
            Số tiền (đ)
            <input
              inputMode="numeric"
              value={amount}
              onChange={(e) => {
                setAmount(e.target.value.replace(/[^\d.]/g, ""));
              }}
              aria-invalid={err("amount") ? true : undefined}
            />
            {err("amount") && <span className="text-sm text-danger">{err("amount")}</span>}
          </label>
          <label className="filter-field">
            Điểm
            <input
              inputMode="numeric"
              value={points}
              onChange={(e) => {
                setPoints(e.target.value.replace(/\D/g, "").slice(0, 3));
              }}
              aria-invalid={pointsError ? true : undefined}
            />
            {pointsError && <span className="text-sm text-danger">{pointsError}</span>}
          </label>
        </div>
        <label className="filter-field">
          Lý do
          <textarea
            rows={2}
            maxLength={500}
            required
            value={reason}
            onChange={(e) => {
              setReason(e.target.value);
            }}
            aria-invalid={reasonError ? true : undefined}
          />
          {reasonError && <span className="text-sm text-danger">{reasonError}</span>}
        </label>
        <DateField
          label="Ngày"
          value={on}
          max={today}
          onChange={setOn}
          invalid={Boolean(err("occurred_on"))}
          describedBy={err("occurred_on") ? "adjustment-date-error" : undefined}
        />
        {err("occurred_on") && (
          <p id="adjustment-date-error" className="text-sm text-danger">
            Ngày: {err("occurred_on")}
          </p>
        )}
        {/* Derived from the month being viewed, not an input: said as a sentence. */}
        {err("corrects_month") && <p className="text-sm text-danger">{err("corrects_month")}</p>}
        {save.isError && (
          <p role="alert" className="text-sm text-danger">
            {messageFor(save.error)}
          </p>
        )}
      </form>
    </BottomSheet>
  );
}
