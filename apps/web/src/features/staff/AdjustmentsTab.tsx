import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { DateField } from "../../components/DateField";
import { ResponsiveTable, type Column } from "../../components/ResponsiveTable";
import { fieldMessage, messageFor } from "../../lib/errors";
import { EmptyState, ErrorState, RowsSkeleton } from "../reports/ui";
import { activeStaffQuery } from "../sales/api";
import { adjustmentsQuery, createAdjustment, invalidateStaff, type Adjustment } from "./api";
import { ClosedBanner, dayLabel, vnd } from "./ui";

/** Thưởng / phạt: the month's log, newest first; recording is append-only (a mistake is offset). */
export function AdjustmentsTab({ month, today }: { month: string; today: string }) {
  const log = useQuery(adjustmentsQuery(month));
  const staff = useQuery(activeStaffQuery);
  const [open, setOpen] = useState(false);

  if (log.isPending) return <RowsSkeleton rows={4} />;
  if (log.isError) return <ErrorState error={log.error} onRetry={() => void log.refetch()} />;
  const names = new Map((staff.data ?? []).map((s) => [s.id, s.full_name]));
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
  ];

  return (
    <div className="grid content-start gap-4">
      {log.data.closed && <ClosedBanner />}
      <div>
        <button
          type="button"
          className="btn btn-primary"
          onClick={() => {
            setOpen(true);
          }}
        >
          + Ghi thưởng / phạt
        </button>
      </div>
      {log.data.items.length === 0 ? (
        <EmptyState title="Chưa có khoản thưởng hay phạt nào trong tháng này" />
      ) : (
        <ResponsiveTable caption="Sổ thưởng phạt" rows={log.data.items} rowKey={(a) => a.id} columns={columns} />
      )}
      {open && (
        <AdjustmentForm
          today={today}
          closedMonth={log.data.closed ? log.data.month : null}
          staff={staff.data ?? []}
          onClose={() => {
            setOpen(false);
          }}
        />
      )}
    </div>
  );
}

function AdjustmentForm({
  today,
  closedMonth,
  staff,
  onClose,
}: {
  today: string;
  closedMonth: string | null;
  staff: { id: string; full_name: string }[];
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const [key] = useState(() => crypto.randomUUID());
  const [staffId, setStaffId] = useState(staff[0]?.id ?? "");
  const [kind, setKind] = useState<"bonus" | "penalty">("bonus");
  const [amount, setAmount] = useState("");
  const [points, setPoints] = useState("");
  const [reason, setReason] = useState("");
  const [on, setOn] = useState(today);
  const save = useMutation({
    mutationFn: () =>
      createAdjustment(
        {
          staff_id: staffId,
          kind,
          amount: amount ? Number(amount.replace(/\D/g, "")) : null,
          points: points ? Number(points) : null,
          reason: reason.trim(),
          occurred_on: on,
          // Viewing a closed month: the entry goes into today's month and names the closed one.
          corrects_month: closedMonth,
        },
        key,
      ),
    onSuccess: async () => {
      await invalidateStaff(queryClient);
      onClose();
    },
  });
  const err = (f: string) => fieldMessage(save.error, f);
  const formId = "adjustment-form";
  return (
    <BottomSheet
      open
      onClose={onClose}
      title="Ghi thưởng / phạt"
      description={closedMonth ? `Điều chỉnh cho tháng ${closedMonth} (đã chốt), ghi vào tháng hiện tại` : undefined}
      footer={
        <button type="submit" form={formId} className="btn btn-primary" disabled={save.isPending || !staffId || !reason.trim() || (!amount && !points)}>
          Lưu
        </button>
      }
    >
      <form
        id={formId}
        className="grid gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          save.mutate();
        }}
      >
        <label className="filter-field">
          Nhân viên
          <select
            value={staffId}
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
            />
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
          />
        </label>
        <DateField label="Ngày" value={on} max={today} onChange={setOn} />
        {save.isError && (
          <p role="alert" className="text-sm text-danger">
            {messageFor(save.error)}
          </p>
        )}
      </form>
    </BottomSheet>
  );
}
