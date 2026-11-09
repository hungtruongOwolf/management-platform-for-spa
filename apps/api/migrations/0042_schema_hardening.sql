-- 0042: hardening from the code + schema audit of 2026-11-09. Nothing here changes what the app
-- does today; each item closes a hole a future bug could fall through.

-- ---------------------------------------------------------------- "does this login belong to another spa?"
-- ADR-0016: a spa never resets the password of a login that also belongs to another spa (the
-- password opens that spa too). The check used `tenants_of_user`, which lists only ACTIVE
-- memberships of ACTIVE spas, so a login locked in spa B, or a suspended spa B, slipped through:
-- once B unlocks it, A's owner knows the password. Any membership elsewhere counts. Without a
-- tenant in the transaction every membership counts (fail closed).
CREATE FUNCTION app.user_in_other_spa(p_user_id uuid) RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = app, pg_temp AS $$
  SELECT EXISTS (
    SELECT 1 FROM app.membership m
    WHERE m.user_id = p_user_id AND m.tenant_id IS DISTINCT FROM app.current_tenant()
  )
$$;
REVOKE ALL ON FUNCTION app.user_in_other_spa(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app.user_in_other_spa(uuid) TO app_rw;

-- ---------------------------------------------------------------- purge floors
-- The purge functions trusted their argument. The job passes constants (48 hours, 30 days), but a
-- wrong caller passing '0' would drop every idempotency record (a retried payment would be charged
-- twice, db review §6 "never shorten 48 h"), and a negative interval would delete live sessions.
-- CREATE OR REPLACE keeps the owner and the grants (REVOKE PUBLIC, EXECUTE to app_rw).
CREATE OR REPLACE FUNCTION app.purge_idempotency_records(older_than interval) RETURNS bigint
LANGUAGE sql SECURITY DEFINER SET search_path = app, pg_temp AS $$
  WITH deleted AS (
    DELETE FROM app.idempotency_record
    WHERE created_at < now() - greatest(older_than, interval '48 hours')
    RETURNING 1
  )
  SELECT count(*) FROM deleted
$$;

CREATE OR REPLACE FUNCTION app.purge_sessions(older_than interval) RETURNS bigint
LANGUAGE sql VOLATILE SECURITY DEFINER SET search_path = app, pg_temp AS $$
  WITH deleted AS (
    DELETE FROM app.session
    WHERE (revoked_at IS NOT NULL AND revoked_at < now() - greatest(older_than, interval '1 day'))
       OR idle_expires_at < now() - greatest(older_than, interval '1 day')
    RETURNING 1
  )
  SELECT count(*) FROM deleted
$$;

-- ---------------------------------------------------------------- narrower grants
-- app_rw had UPDATE/DELETE on whole tables where the code changes one column or nothing:
-- the only UPDATE of app.membership sets `status` (org lock/unlock), the only UPDATE of app.role
-- sets `follows_template` (platform role edits), nothing updates app.branch and nothing deletes a
-- role. One buggy statement could otherwise move a membership to another login, or delete a role
-- and cascade its grants away (the owner would lose staff.manage). Operator and provisioning
-- writes run as the owner (definer functions / CLI) and are not affected.
REVOKE DELETE ON app.role FROM app_rw;
REVOKE UPDATE ON app.role FROM app_rw;
GRANT UPDATE (follows_template) ON app.role TO app_rw;
REVOKE UPDATE ON app.membership FROM app_rw;
GRANT UPDATE (status) ON app.membership TO app_rw;
REVOKE UPDATE ON app.branch FROM app_rw;

-- ---------------------------------------------------------------- integrity
-- A package line's item must belong to the line's package: two independent FKs allowed a bug to
-- book package A's line on package B's item (A's history would show B's sessions). One-off lines
-- have both ids NULL and are not checked (MATCH SIMPLE). NOT VALID here, validated in 0043
-- without blocking writes (same pattern as 0036/0037).
ALTER TABLE app.customer_package_item
  ADD CONSTRAINT customer_package_item_tenant_package_id_key UNIQUE (tenant_id, customer_package_id, id);
ALTER TABLE app.visit_line
  ADD CONSTRAINT visit_line_item_of_package_fkey
  FOREIGN KEY (tenant_id, customer_package_id, customer_package_item_id)
  REFERENCES app.customer_package_item (tenant_id, customer_package_id, id) NOT VALID;

-- The one actor column 0036 missed.
ALTER TABLE app.appointment_setting
  ADD CONSTRAINT appointment_setting_updated_by_fkey
  FOREIGN KEY (updated_by) REFERENCES app.user_account (id) NOT VALID;
