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
 * `minutes` ago, so the next 🔒 action deterministically asks for re-auth (window: 5 minutes).
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
