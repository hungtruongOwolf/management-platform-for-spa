-- Read-only: payload sizes behind the storage projections (db review 2026-11). Sizes only, never
-- contents: no customer data leaves the database.

\echo '== audit_log: rows and bytes per action'
SELECT action, count(*) AS n,
       round(avg(pg_column_size(a.*))) AS avg_row_bytes,
       round(avg(coalesce(pg_column_size(before), 0) + coalesce(pg_column_size(after), 0))) AS avg_json_bytes,
       max(coalesce(pg_column_size(before), 0) + coalesce(pg_column_size(after), 0)) AS max_json_bytes
FROM app.audit_log a GROUP BY action ORDER BY n DESC LIMIT 30;

\echo '== audit_log: per day'
SELECT count(*) AS rows, count(DISTINCT at::date) AS days, min(at)::date AS first, max(at)::date AS last
FROM app.audit_log;

\echo '== idempotency_record: response sizes'
SELECT count(*) AS n, round(avg(pg_column_size(response_body))) AS avg_body, max(pg_column_size(response_body)) AS max_body,
       min(created_at) AS oldest
FROM app.idempotency_record;

\echo '== session: live vs dead rows'
SELECT count(*) AS total,
       count(*) FILTER (WHERE revoked_at IS NOT NULL) AS revoked,
       count(*) FILTER (WHERE revoked_at IS NULL AND (idle_expires_at < now() OR absolute_expires_at < now())) AS expired,
       count(*) FILTER (WHERE revoked_at IS NULL AND idle_expires_at >= now() AND absolute_expires_at >= now()) AS live,
       min(created_at)::date AS oldest
FROM app.session;

\echo '== ciphertext sizes'
SELECT 'visit.note_ct' AS col, count(note_ct) AS n, round(avg(length(note_ct))) AS avg_bytes, max(length(note_ct)) AS max_bytes FROM app.visit
UNION ALL SELECT 'sale.note_ct', count(note_ct), round(avg(length(note_ct))), max(length(note_ct)) FROM app.sale
UNION ALL SELECT 'customer.health_note_ct', count(health_note_ct), round(avg(length(health_note_ct))), max(length(health_note_ct)) FROM app.customer;

\echo '== average row width per big table'
SELECT 'visit' AS t, round(avg(pg_column_size(x.*))) AS avg_row FROM app.visit x
UNION ALL SELECT 'visit_line', round(avg(pg_column_size(x.*))) FROM app.visit_line x
UNION ALL SELECT 'sale', round(avg(pg_column_size(x.*))) FROM app.sale x
UNION ALL SELECT 'sale_line', round(avg(pg_column_size(x.*))) FROM app.sale_line x
UNION ALL SELECT 'payment', round(avg(pg_column_size(x.*))) FROM app.payment x
UNION ALL SELECT 'customer', round(avg(pg_column_size(x.*))) FROM app.customer x
UNION ALL SELECT 'appointment', round(avg(pg_column_size(x.*))) FROM app.appointment x;
