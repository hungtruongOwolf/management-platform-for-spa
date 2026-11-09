import type { FollowUp, FollowUpChannel, FollowUpOutcome } from "@vpyd/api-client";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { DateField } from "../../components/DateField";
import { StickyActions } from "../../components/StickyActions";
import { ApiError } from "../../lib/api";
import { fieldMessage, messageFor } from "../../lib/errors";
import { spaToday } from "../corrections/api";
import { shiftDays } from "../reports/period";
import {
  CHANNEL_LABEL,
  CONTACT_MAX_DAYS_BACK,
  NEXT_TRY_DAYS,
  NO_NEXT_TRY_HINT,
  OUTCOME_LABEL,
  createFollowUp,
  invalidateAfterFollowUp,
  putFollowUp,
} from "./followUps";

const NEXT_TRY_ERRORS: Record<string, string> = {
  before_contact: "Ngày hẹn lại phải từ ngày liên hệ trở đi.",
  too_far: "Ngày hẹn lại tối đa 365 ngày sau ngày liên hệ.",
};

/** "08/11" (vi-VN Intl would print "08-11"). */
const shortDate = (iso: string) => `${iso.slice(8, 10)}/${iso.slice(5, 7)}`;

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
    onSuccess: (saved) => {
      putFollowUp(queryClient, customerId, saved);
      void invalidateAfterFollowUp(queryClient, customerId);
      onDone(saved);
    },
  });
  // Field codes are shared across screens (`too_far` also means "booking > 90 days"): the
  // next-try date's own two codes are worded here (crm follow_up.rs validate).
  const err = (field: string) => {
    const code = mutation.error instanceof ApiError ? mutation.error.fieldCode(field) : undefined;
    return (field === "next_follow_up_on" && code !== undefined && NEXT_TRY_ERRORS[code]) || fieldMessage(mutation.error, field);
  };

  return (
    <form
      aria-label={`Ghi lần liên hệ với ${customerName}`}
      noValidate
      className="grid gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        if (!nextTooEarly && contactedOn && !mutation.isPending) mutation.mutate();
      }}
    >
      <div className="grid gap-4 sm:grid-cols-2">
        <div className="field">
          <DateField
            label="Ngày liên hệ"
            value={contactedOn}
            min={minDay}
            max={today}
            onChange={(iso) => {
              setContactedOn(iso || today);
            }}
          />
          {err("contacted_on") && <span className="field-error">{err("contacted_on")}</span>}
        </div>
        <label className="field">
          Cách liên hệ
          <select
            className="input"
            value={channel}
            onChange={(e) => {
              setChannel(e.target.value as FollowUpChannel);
            }}
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
        <legend className="mb-1.5 text-sm font-medium">Kết quả</legend>
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
      <label className="field">
        Ghi chú (không bắt buộc)
        <textarea
          className="input"
          value={note}
          maxLength={500}
          rows={2}
          placeholder="Ví dụ: hẹn cuối tuần, đang đi công tác…"
          onChange={(e) => {
            setNote(e.target.value);
          }}
        />
      </label>
      <div className="grid gap-2">
        <DateField
          label="Hẹn liên hệ lại"
          value={next}
          min={contactedOn}
          max={shiftDays(contactedOn, 365)}
          describedBy={`${id}-next-hint`}
          invalid={nextTooEarly || Boolean(err("next_follow_up_on"))}
          onChange={(iso) => {
            setNextChoice(iso);
          }}
        />
        <p id={`${id}-next-hint`} className="flex flex-wrap items-center gap-x-2 text-sm text-text-muted">
          {nextTooEarly ? (
            <span className="field-error">Ngày hẹn lại phải từ ngày liên hệ trở đi.</span>
          ) : suggestion ? (
            <>
              Gợi ý: {shortDate(suggestion)} (sau {NEXT_TRY_DAYS[outcome]} ngày)
              {next !== suggestion && (
                <button
                  type="button"
                  className="btn btn-ghost"
                  onClick={() => {
                    setNextChoice(undefined);
                  }}
                >
                  Dùng gợi ý
                </button>
              )}
            </>
          ) : (
            NO_NEXT_TRY_HINT[outcome]
          )}
          {next !== "" && (
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => {
                setNextChoice("");
              }}
            >
              Không hẹn
            </button>
          )}
        </p>
        {err("next_follow_up_on") && <span className="field-error">{err("next_follow_up_on")}</span>}
      </div>
      {mutation.isError && !err("contacted_on") && !err("next_follow_up_on") && (
        <p role="alert" className="alert">
          {messageFor(mutation.error)}
        </p>
      )}
      <StickyActions>
        <button type="button" className="btn btn-secondary" onClick={onCancel}>
          Hủy
        </button>
        <button type="submit" className="btn btn-primary" aria-busy={mutation.isPending || undefined} aria-disabled={nextTooEarly || undefined}>
          Lưu lần liên hệ
        </button>
      </StickyActions>
    </form>
  );
}
