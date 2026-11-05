import type { FollowUp, FollowUpChannel, FollowUpCreate, FollowUpList, FollowUpOutcome } from "@vpyd/api-client";
import { queryOptions, type QueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";

/** Re-engagement follow-ups (EM round 3 item 16): who was contacted, how, and when to try again. */

export const CHANNEL_LABEL: Record<FollowUpChannel, string> = {
  phone: "Gọi điện",
  zalo: "Zalo",
  in_person: "Gặp trực tiếp",
  other: "Khác",
};

export const OUTCOME_LABEL: Record<FollowUpOutcome, string> = {
  will_come: "Hẹn sẽ quay lại",
  no_answer: "Không nghe máy",
  not_interested: "Chưa có nhu cầu",
  other: "Khác",
};

/**
 * Suggested next try per outcome (lead decision 2026-11-05, the only copy in the web app): days
 * after the contact, or `null` for no date. Pre-filled as soon as the outcome is picked; a date
 * the user types always wins. The server applies the same rule when no date is sent.
 */
export const NEXT_TRY_DAYS: Record<FollowUpOutcome, number | null> = {
  no_answer: 3,
  will_come: 7,
  not_interested: null,
  other: null,
};

/** Hint shown instead of a suggestion, per outcome without one. */
export const NO_NEXT_TRY_HINT: Partial<Record<FollowUpOutcome, string>> = {
  not_interested: "Không nhắc lại trong 90 ngày.",
};

/** The API keeps contacts up to 30 days back, never in the future. */
export const CONTACT_MAX_DAYS_BACK = 30;

export function followUpsQuery(customerId: string) {
  return queryOptions({
    queryKey: ["customers", "follow-ups", customerId],
    queryFn: async () => (await api(`/customers/${customerId}/follow-ups`)) as FollowUpList,
    staleTime: 30_000,
  });
}

export async function createFollowUp(customerId: string, body: FollowUpCreate, idempotencyKey: string): Promise<FollowUp> {
  return (await api(`/customers/${customerId}/follow-ups`, {
    method: "POST",
    body,
    headers: { "Idempotency-Key": idempotencyKey },
  })) as FollowUp;
}

/** After a contact is recorded: the overdue list (every filter) and that customer's profile. */
export async function invalidateAfterFollowUp(queryClient: QueryClient, customerId: string): Promise<void> {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: ["customers", "inactive"] }),
    queryClient.invalidateQueries({ queryKey: ["customers", "follow-ups", customerId] }),
    queryClient.invalidateQueries({ queryKey: ["customers", "summary", customerId] }),
  ]);
}
