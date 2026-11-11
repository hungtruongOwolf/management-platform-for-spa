-- 0051: "Nhật ký thao tác", the owner's screen over the spa's own audit trail (ADR-0021, EM
-- 2026-11-11). Read-only over app.audit_log: no new table, no new grant (app_rw already has
-- SELECT + INSERT under the tenant_isolation policy of 0001).

-- ---------------------------------------------------------------- indexes
-- `GET /audit-log` pages by (at, id) newest first inside one spa and a time range. 0038's
-- (tenant_id, at DESC) served the console's spa view (same order); this one also carries the
-- tie-breaker, so a page boundary inside one instant is an index seek, and replaces it.
CREATE INDEX audit_log_tenant_at_id_idx ON app.audit_log (tenant_id, at DESC, id DESC);
DROP INDEX app.audit_log_tenant_at_idx;

-- The person filter: one member's entries over a range. Platform / system entries are filtered
-- by actor_kind on the index above (they are few).
CREATE INDEX audit_log_tenant_actor_at_idx ON app.audit_log (tenant_id, actor_user_id, at DESC, id DESC)
  WHERE actor_user_id IS NOT NULL;

-- ---------------------------------------------------------------- audit.read is owner-only
-- The trail names who did what to whom (customer names and, from now on, their old / new phone):
-- only the owner role may hold `audit.read` (platform::roles::OWNER_ONLY). The code has refused
-- to grant it to another role since this change; rows an operator stored earlier are removed so
-- the rule holds for every spa. Idempotent.
DELETE FROM app.role_template_permission
WHERE permission = 'audit.read' AND role_key <> 'owner';

DELETE FROM app.role_permission rp
USING app.role r
WHERE r.tenant_id = rp.tenant_id AND r.id = rp.role_id
  AND rp.permission = 'audit.read' AND NOT (r.is_system AND r.key = 'owner');
