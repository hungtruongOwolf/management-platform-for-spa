-- 0026: a leaver keeps a login to their own past pay (EM 2026-11-07 Q-S6, ADR-0014).
-- Ending employment no longer disables the membership: the API derives, on every request, that
-- a member whose linked staff profile has left holds only `staff.read_own_past` (their own
-- attendance and payslips of the months they worked). The owner may still lock the login when
-- ending employment (`lock_login`), which disables the membership as before.
--
-- Until now `POST /staff/{id}/end` was the only writer of `membership.status = 'disabled'`, so
-- every disabled membership linked to a left profile was locked by that rule, not by choice:
-- give those people the new read-only access. Depends on 0002 (membership), 0017 (staff.user_id),
-- 0025 (staff roster).
UPDATE app.membership m
SET status = 'active'
WHERE m.status = 'disabled'
  AND EXISTS (
    SELECT 1 FROM app.staff s
    WHERE s.tenant_id = m.tenant_id AND s.user_id = m.user_id AND s.status = 'left'
  );
