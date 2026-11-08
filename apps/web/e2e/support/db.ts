/**
 * Test-only adjustments of the E2E database, for states a browser cannot reach quickly (e.g. a
 * password confirmation older than the re-auth window). Connects as the schema owner of the
 * dedicated E2E database; never used by the app or against dev/prod data.
 */
import postgres from "postgres";
import { stack } from "../stack";

/** E.164 form of a Vietnamese mobile number as the run stores it (`090…` → `+8490…`). */
function e164(phone: string): string {
  return `+84${phone.replace(/^0/, "")}`;
}

/**
 * Makes every live session of the account behave as if its password was last confirmed
 * `minutes` ago (outside the 5-minute re-auth window), e.g. to prove an action does not ask.
 */
export async function ageRecentAuth(
  phone: string,
  minutes = 10,
): Promise<void> {
  const sql = postgres(stack.ownerDatabaseUrl(), {
    max: 1,
    onnotice: () => undefined,
  });
  try {
    const rows = await sql`
      UPDATE app.session s
         SET recent_auth_at = now() - make_interval(mins => ${minutes})
        FROM app.user_account u
       WHERE s.user_id = u.id AND u.phone_e164 = ${e164(phone)} AND s.revoked_at IS NULL
      RETURNING s.id`;
    if (rows.length === 0) throw new Error(`no live session for ${phone}`);
  } finally {
    await sql.end();
  }
}

/**
 * Moves a customer's visits and sales `days` back, so the customer counts as "lâu không quay lại"
 * without waiting (US5). Only completed visits and open sales: voided rows are final (0009).
 */
export async function backdateCustomer(customerId: string, days: number): Promise<void> {
  const sql = postgres(stack.ownerDatabaseUrl(), {
    max: 1,
    onnotice: () => undefined,
  });
  try {
    const visits = await sql`
      UPDATE app.visit SET business_date = business_date - ${days}::int
       WHERE customer_id = ${customerId} AND status = 'completed'
      RETURNING id`;
    if (visits.length === 0) throw new Error(`no visit to backdate for ${customerId}`);
    await sql`
      UPDATE app.sale SET business_date = business_date - ${days}::int
       WHERE customer_id = ${customerId} AND status = 'open'`;
  } finally {
    await sql.end();
  }
}

/**
 * Moves bookings onto `today` (the spa's day, `YYYY-MM-DD`) at the same clock time (F12,
 * 2026-11-08): a slot of today already over can no longer be booked, yet the desk flows
 * (arrive, record) happen today. Book on a later day through the API, then call this: the spec
 * runs the same at any hour.
 */
export async function bookingsToToday(ids: string[], today: string): Promise<void> {
  const sql = postgres(stack.ownerDatabaseUrl(), {
    max: 1,
    onnotice: () => undefined,
  });
  try {
    const moved = await sql`
      UPDATE app.appointment
         SET starts_at = starts_at - make_interval(days => business_date - ${today}::date),
             ends_at = ends_at - make_interval(days => business_date - ${today}::date),
             business_date = ${today}::date
       WHERE id = ANY(${ids}::uuid[])
      RETURNING id`;
    if (moved.length !== ids.length) throw new Error(`moved ${String(moved.length)} of ${String(ids.length)} bookings`);
  } finally {
    await sql.end();
  }
}
