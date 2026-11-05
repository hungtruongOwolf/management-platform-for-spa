import type { FollowUp, FollowUpChannel, FollowUpOutcome } from "@vpyd/api-client";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { fieldMessage, messageFor } from "../../lib/errors";
import { spaToday } from "../corrections/api";
import { shiftDays } from "../reports/period";
import {
  CHANNEL_LABEL,
  CONTACT_MAX_DAYS_BACK,
  NEXT_TRY_DAYS,
  OUTCOME_LABEL,
  createFollowUp,
  invalidateAfterFollowUp,
} from "./followUps";

const dm = new Intl.DateTimeFormat("vi-VN", { day: "2-digit", month: "2-digit", timeZone: "UTC" });
const shortDate = (iso: string) => dm.format(new Date(`${iso}T00:00:00Z`));

/** The suggested next try for an outcome, counted from the contact day. */
export function suggestedNextTry(outcome: FollowUpOutcome, contactedOn: string): string | null {
  const days = NEXT_TRY_DAYS[outcome];
  return days === null ? null : shiftDays(contactedOn, days);
}

/**
 * "Đã liên hệ": records one contact with a customer who has not come back. Date (today by
 * default), channel, outcome, optional note, and the next try, pre-filled with a suggestion until
 * the user picks a date. Plain state, no form library: four fields, and the server validates again.
 */
export function FollowUpForm({
  customerId,
  customerName,
  onDone,
  onCancel,
}: {
  customerId: string;
  customerName: string;
  onDone: (saved: FollowUp) => void;
  onCancel: () => void;
}) {
  const id = useId();
  const today = spaToday();
  const queryClient = useQueryClient();
  const [idempotencyKey] = useState(() => crypto.randomUUID());
  const [contactedOn, setContactedOn] = useState(today);
  const [channel, setChannel] = useState<FollowUpChannel>("phone");
  const [outcome, setOutcome] = useState<FollowUpOutcome>("no_answer");
  const [note, setNote] = useState("");
  // `undefined` = follow the suggestion; a string (possibly empty) = the user's own choice.
  const [nextChoice, setNextChoice] = useState<string | undefined>(undefined);
  const suggestion = suggestedNextTry(outcome, contactedOn);
  const next = nextChoice ?? suggestion ?? "";
  const minDay = shiftDays(today, -CONTACT_MAX_DAYS_BACK);
  const nextTooEarly = next !== "" && next < contactedOn;

  const mutation = useMutation({
    mutationFn: () =>
      createFollowUp(
        customerId,
        {
          contacted_on: contactedOn,
          channel,
          outcome,
          note: note.trim() === "" ? null : note.trim(),
          next_follow_up_on: next === "" ? null : next,
        },
        idempotencyKey,
      ),
    onSuccess: async (saved) => {
      await invalidateAfterFollowUp(queryClient, customerId);
      onDone(saved);
    },
  });
  const err = (field: string) => fieldMessage(mutation.error, field);

  return (
    <form
      aria-label={`Ghi lần liên hệ với ${customerName}`}
      noValidate
      className="grid gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        if (!nextTooEarly && contactedOn) mutation.mutate();
      }}
    >
      <div className="grid gap-4 sm:grid-cols-2">
        <label className="filter-field">
          Ngày liên hệ
          <input
            type="date"
            required
            value={contactedOn}
            min={minDay}
            max={today}
            onChange={(e) => {
              setContactedOn(e.target.value || today);
            }}
            aria-invalid={err("contacted_on") ? true : undefined}
          />
          {err("contacted_on") && <span className="text-sm text-danger">{err("contacted_on")}</span>}
        </label>
        <label className="filter-field">
          Cách liên hệ
          <select
            value={channel}
            onChange={(e) => {
              setChannel(e.target.value as FollowUpChannel);
            }}
            className="min-h-11 rounded-[10px] bg-surface px-3 text-sm text-text"
          >
            {(Object.keys(CHANNEL_LABEL) as FollowUpChannel[]).map((c) => (
              <option key={c} value={c}>
                {CHANNEL_LABEL[c]}
              </option>
            ))}
          </select>
        </label>
      </div>
      <fieldset className="grid gap-2">
        <legend className="mb-1 text-xs text-text-muted">Kết quả</legend>
        <div className="grid gap-2 sm:grid-cols-2">
          {(Object.keys(OUTCOME_LABEL) as FollowUpOutcome[]).map((o) => (
            <label key={o} className="choice">
              <input
                type="radio"
                name={`${id}-outcome`}
                value={o}
                checked={outcome === o}
                onChange={() => {
                  setOutcome(o);
                }}
              />
              {OUTCOME_LABEL[o]}
            </label>
          ))}
        </div>
      </fieldset>
      <label className="filter-field">
        Ghi chú (không bắt buộc)
        <textarea
          value={note}
          maxLength={500}
          rows={2}
          placeholder="Ví dụ: hẹn cuối tuần, đang đi công tác…"
          onChange={(e) => {
            setNote(e.target.value);
          }}
          className="min-h-16 rounded-[10px] bg-surface px-3 py-2 text-sm text-text"
        />
      </label>
      <div className="grid gap-2">
        <label className="filter-field">
          Hẹn liên hệ lại
          <input
            type="date"
            value={next}
            min={contactedOn}
            max={shiftDays(contactedOn, 365)}
            aria-describedby={`${id}-next-hint`}
            aria-invalid={nextTooEarly || err("next_follow_up_on") ? true : undefined}
            onChange={(e) => {
              setNextChoice(e.target.value);
            }}
          />
        </label>
        <p id={`${id}-next-hint`} className="flex flex-wrap items-center gap-2 text-sm text-text-muted">
          {nextTooEarly ? (
            <span className="text-danger">Ngày hẹn lại phải từ ngày liên hệ trở đi.</span>
          ) : suggestion ? (
            <>
              Gợi ý: {shortDate(suggestion)} (sau {NEXT_TRY_DAYS[outcome]} ngày)
              {next !== suggestion && (
                <button
                  type="button"
                  className="font-semibold text-primary underline"
                  onClick={() => {
                    setNextChoice(undefined);
                  }}
                >
                  Dùng gợi ý
                </button>
              )}
            </>
          ) : (
            "Khách chưa có nhu cầu: không cần hẹn lại."
          )}
          {next !== "" && (
            <button
              type="button"
              className="font-semibold text-primary underline"
              onClick={() => {
                setNextChoice("");
              }}
            >
              Không hẹn
            </button>
          )}
        </p>
        {err("next_follow_up_on") && <span className="text-sm text-danger">{err("next_follow_up_on")}</span>}
      </div>
      {mutation.isError && !err("contacted_on") && !err("next_follow_up_on") && (
        <p role="alert" className="rounded-[10px] bg-danger-soft p-3 text-sm text-danger">
          {messageFor(mutation.error)}
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        <button type="submit" className="btn btn-primary" disabled={mutation.isPending || nextTooEarly}>
          {mutation.isPending ? "Đang lưu…" : "Lưu lần liên hệ"}
        </button>
        <button type="button" className="btn btn-secondary" onClick={onCancel}>
          Hủy
        </button>
      </div>
    </form>
  );
}
