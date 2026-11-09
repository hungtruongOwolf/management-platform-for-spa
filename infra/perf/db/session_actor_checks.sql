-- Behaviour checks of app.session_actor (0034, ADR-0017) on fixtures, at the SQL level: every
-- rule the function enforces must show. Prints one row per failed check (0 rows = all pass).
-- Run inside a transaction that is rolled back, on a throwaway database migrated to >= 0034:
--   (echo BEGIN; cat [mutant.sql] infra/perf/db/session_actor_checks.sql; echo ROLLBACK) | psql …
-- With a mutant (CREATE OR REPLACE FUNCTION with one rule broken) first, at least one check must
-- fail: that is the mutation check of the definer (db review A3).

CREATE TEMP TABLE f AS SELECT
  gen_random_uuid() AS ta, gen_random_uuid() AS tb, gen_random_uuid() AS ts,
  gen_random_uuid() AS ra, gen_random_uuid() AS rb, gen_random_uuid() AS rs,
  gen_random_uuid() AS u1, gen_random_uuid() AS u2, gen_random_uuid() AS u3,
  gen_random_uuid() AS u4, gen_random_uuid() AS u5, gen_random_uuid() AS u6;

INSERT INTO app.tenant (id, name, slug, status)
SELECT ta, 'Spa A', 'chk-a', 'active' FROM f UNION ALL
SELECT tb, 'Spa B', 'chk-b', 'active' FROM f UNION ALL
SELECT ts, 'Spa S', 'chk-s', 'suspended' FROM f;
INSERT INTO app.branch (id, tenant_id, name) SELECT gen_random_uuid(), t, 'Cơ sở' FROM f, unnest(ARRAY[ta, tb, ts]) t;
INSERT INTO app.role (id, tenant_id, key, name) SELECT ra, ta, 'manager', 'Quản lý' FROM f UNION ALL
  SELECT rb, tb, 'technician', 'KTV' FROM f UNION ALL SELECT rs, ts, 'manager', 'Quản lý' FROM f;
INSERT INTO app.role_permission (tenant_id, role_id, permission)
SELECT ta, ra, p FROM f, unnest(ARRAY['visit.record', 'customer.read']) p UNION ALL
SELECT tb, rb, 'staff.read_own' FROM f UNION ALL
SELECT ts, rs, 'visit.record' FROM f;
INSERT INTO app.user_account (id, phone_e164, full_name, password_hash, status, must_change_password)
SELECT u1, '+84900000001', 'U1', '$argon2id$x', 'active', false FROM f UNION ALL
SELECT u2, '+84900000002', 'U2', '$argon2id$x', 'disabled', false FROM f UNION ALL
SELECT u3, '+84900000003', 'U3', '$argon2id$x', 'active', true FROM f UNION ALL
SELECT u4, '+84900000004', 'U4', '$argon2id$x', 'active', false FROM f UNION ALL
SELECT u5, '+84900000005', 'U5', '$argon2id$x', 'active', false FROM f UNION ALL
SELECT u6, '+84900000006', 'U6', '$argon2id$x', 'active', false FROM f;
-- Memberships: u1 manager in A, technician in B, manager in S; u2/u3/u4 managers in A; u5's A
-- membership disabled; u6 member of B only.
CREATE TEMP TABLE mem AS
SELECT gen_random_uuid() AS id, x.t, x.u, x.r, x.st FROM f, LATERAL (VALUES
  (ta, u1, ra, 'active'), (tb, u1, rb, 'active'), (ts, u1, rs, 'active'),
  (ta, u2, ra, 'active'), (ta, u3, ra, 'active'), (ta, u4, ra, 'active'),
  (ta, u5, ra, 'disabled'), (tb, u6, rb, 'active')) x(t, u, r, st);
INSERT INTO app.membership (id, tenant_id, user_id, status) SELECT id, t, u, st FROM mem;
INSERT INTO app.membership_role (tenant_id, membership_id, role_id) SELECT t, id, r FROM mem;
-- u4's linked staff profile has left (ADR-0014).
INSERT INTO app.staff (id, tenant_id, branch_id, full_name, position, status, joined_on, left_on, user_id)
SELECT gen_random_uuid(), ta, (SELECT id FROM app.branch WHERE tenant_id = ta), 'U4', 'Quản lý', 'left',
       current_date - 100, current_date - 1, u4 FROM f;

-- Sessions, by name; the token hash is sha256 of the name.
CREATE TEMP TABLE ses (name text, u uuid, t uuid, revoked boolean, idle interval, absolute interval, mfa boolean);
INSERT INTO ses SELECT x.* FROM f, LATERAL (VALUES
  ('live', u1, ta, false, interval '1 hour', interval '1 day', false),
  ('revoked', u1, ta, true, interval '1 hour', interval '1 day', false),
  ('idle_expired', u1, ta, false, interval '-1 minute', interval '1 day', false),
  ('absolute_expired', u1, ta, false, interval '1 hour', interval '-1 minute', false),
  ('disabled_account', u2, ta, false, interval '1 hour', interval '1 day', false),
  ('must_change', u3, ta, false, interval '1 hour', interval '1 day', false),
  ('former_staff', u4, ta, false, interval '1 hour', interval '1 day', false),
  ('membership_disabled', u5, ta, false, interval '1 hour', interval '1 day', false),
  ('suspended_spa', u1, ts, false, interval '1 hour', interval '1 day', false),
  ('mfa_pending', u1, ta, false, interval '1 hour', interval '1 day', true),
  ('other_spa_member', u6, ta, false, interval '1 hour', interval '1 day', false),
  ('no_spa', u1, NULL, false, interval '1 hour', interval '1 day', false)
) x(name, u, t, revoked, idle, absolute, mfa);
-- Idle expiry is capped by the absolute one (CHECK idle <= absolute).
INSERT INTO app.session (id, token_hash, user_id, active_tenant_id, idle_expires_at, absolute_expires_at, recent_auth_at, revoked_at, mfa_pending)
SELECT gen_random_uuid(), sha256(convert_to(name, 'UTF8')), u, t,
       least(now() + idle, now() + absolute), now() + absolute, now(),
       CASE WHEN revoked THEN now() END, mfa
FROM ses;

CREATE TEMP TABLE r AS
SELECT s.name, a.* FROM ses s
LEFT JOIN LATERAL app.session_actor(sha256(convert_to(s.name, 'UTF8'))) a ON true;

SELECT check_name FROM (
  SELECT 'live session resolves, member, own spa role only' AS check_name,
         (SELECT member AND permissions = ARRAY['customer.read', 'visit.record'] AND NOT former_staff
                 AND NOT must_change_password AND NOT mfa_pending AND branch_id IS NOT NULL
          FROM r WHERE name = 'live') AS ok
  UNION ALL SELECT 'revoked session resolves to nothing', (SELECT session_id IS NULL FROM r WHERE name = 'revoked')
  UNION ALL SELECT 'idle-expired session resolves to nothing', (SELECT session_id IS NULL FROM r WHERE name = 'idle_expired')
  UNION ALL SELECT 'absolutely expired session resolves to nothing', (SELECT session_id IS NULL FROM r WHERE name = 'absolute_expired')
  UNION ALL SELECT 'disabled account resolves to nothing', (SELECT session_id IS NULL FROM r WHERE name = 'disabled_account')
  UNION ALL SELECT 'temporary password is flagged', (SELECT must_change_password FROM r WHERE name = 'must_change')
  UNION ALL SELECT 'former staff is flagged', (SELECT former_staff AND member FROM r WHERE name = 'former_staff')
  UNION ALL SELECT 'disabled membership is not a member, no permission',
         (SELECT session_id IS NOT NULL AND NOT member AND permissions = '{}' FROM r WHERE name = 'membership_disabled')
  UNION ALL SELECT 'suspended spa grants nothing',
         (SELECT session_id IS NOT NULL AND NOT member AND permissions = '{}' FROM r WHERE name = 'suspended_spa')
  UNION ALL SELECT 'MFA pending is flagged', (SELECT mfa_pending FROM r WHERE name = 'mfa_pending')
  UNION ALL SELECT 'a member of another spa is not a member here',
         (SELECT session_id IS NOT NULL AND NOT member AND permissions = '{}' FROM r WHERE name = 'other_spa_member')
  UNION ALL SELECT 'no active spa, no membership', (SELECT session_id IS NOT NULL AND NOT member FROM r WHERE name = 'no_spa')
  UNION ALL SELECT 'one row per session', (SELECT count(*) = (SELECT count(*) FROM ses) FROM r)
) c WHERE ok IS NOT TRUE;
