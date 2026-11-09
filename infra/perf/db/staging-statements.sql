-- Read-only: what the API (role app_rw) runs, from pg_stat_statements (db review 2026-11).
-- Query text is cut to 160 characters; parameters are already normalised ($1…) by Postgres.

\echo '== totals per role'
SELECT r.rolname, count(*) AS statements, sum(s.calls) AS calls,
       round(sum(s.total_exec_time)::numeric, 1) AS exec_ms,
       round(sum(s.total_plan_time)::numeric, 1) AS plan_ms
FROM extensions.pg_stat_statements s JOIN pg_roles r ON r.oid = s.userid
WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
GROUP BY r.rolname ORDER BY calls DESC;

\echo '== app_rw: most called'
SELECT s.calls, round(s.mean_exec_time::numeric, 3) AS mean_ms, round(s.max_exec_time::numeric, 2) AS max_ms,
       round(s.total_exec_time::numeric, 1) AS total_ms, s.rows / greatest(s.calls, 1) AS rows_per_call,
       s.shared_blks_hit / greatest(s.calls, 1) AS hit_per_call,
       left(regexp_replace(s.query, '\s+', ' ', 'g'), 160) AS query
FROM extensions.pg_stat_statements s JOIN pg_roles r ON r.oid = s.userid
WHERE r.rolname LIKE 'app_rw%'
ORDER BY s.calls DESC LIMIT 40;

\echo '== app_rw: most total time'
SELECT s.calls, round(s.mean_exec_time::numeric, 3) AS mean_ms, round(s.max_exec_time::numeric, 2) AS max_ms,
       round(s.total_exec_time::numeric, 1) AS total_ms,
       s.shared_blks_hit / greatest(s.calls, 1) AS hit_per_call,
       left(regexp_replace(s.query, '\s+', ' ', 'g'), 160) AS query
FROM extensions.pg_stat_statements s JOIN pg_roles r ON r.oid = s.userid
WHERE r.rolname LIKE 'app_rw%'
ORDER BY s.total_exec_time DESC LIMIT 25;

\echo '== app_rw: most buffers per call (scale risk)'
SELECT s.calls, round(s.mean_exec_time::numeric, 3) AS mean_ms,
       s.shared_blks_hit / greatest(s.calls, 1) AS hit_per_call, s.rows / greatest(s.calls, 1) AS rows_per_call,
       left(regexp_replace(s.query, '\s+', ' ', 'g'), 160) AS query
FROM extensions.pg_stat_statements s JOIN pg_roles r ON r.oid = s.userid
WHERE r.rolname LIKE 'app_rw%' AND s.calls >= 5
ORDER BY s.shared_blks_hit / greatest(s.calls, 1) DESC LIMIT 20;
