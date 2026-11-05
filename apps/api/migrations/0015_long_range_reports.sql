-- 0015: owner-only long-range reports (EM Q5, memory/raw/2026-11-05-em-answers-q5-q7.md).
-- Everyone keeps the 2-year cap on month/year reports; owners may also look back 3–5 years.
-- New spas get it from identity::roles::SystemRole::default_permissions (owner = everything).
INSERT INTO app.role_permission (tenant_id, role_id, permission)
SELECT r.tenant_id, r.id, 'report.long_range'
FROM app.role r
WHERE r.is_system AND r.key = 'owner'
ON CONFLICT DO NOTHING;