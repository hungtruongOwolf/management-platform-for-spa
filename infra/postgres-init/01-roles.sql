-- Cluster-level roles (not part of migrations). Runs once when the dev volume is created.
-- Production/Supabase: create the same roles once by hand with strong passwords (runbook in S11).
-- app_rw: the runtime role. Not a table owner, no BYPASSRLS → row-level security always applies.
CREATE ROLE app_rw LOGIN PASSWORD 'app_rw_dev_password' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS;
