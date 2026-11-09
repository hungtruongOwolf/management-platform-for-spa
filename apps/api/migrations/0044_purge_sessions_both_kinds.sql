-- 0044: app.purge_sessions purges operator sessions again. 0041 extended it to app.platform_session;
-- 0042 (written in parallel, before 0041 landed) replaced it with a spa-session-only body to add
-- the 1-day floor, so operator sessions stopped being purged. This is 0041's body with 0042's
-- floor: whatever interval a caller passes, a session that ended less than a day ago, or a live
-- one, is never touched. CREATE OR REPLACE keeps the owner and the grants.
CREATE OR REPLACE FUNCTION app.purge_sessions(older_than interval) RETURNS bigint
LANGUAGE sql VOLATILE SECURITY DEFINER SET search_path = app, pg_temp AS $$
  WITH spa AS (
    DELETE FROM app.session
    WHERE (revoked_at IS NOT NULL AND revoked_at < now() - greatest(older_than, interval '1 day'))
       OR idle_expires_at < now() - greatest(older_than, interval '1 day')
    RETURNING 1
  ), operator AS (
    DELETE FROM app.platform_session
    WHERE (revoked_at IS NOT NULL AND revoked_at < now() - greatest(older_than, interval '1 day'))
       OR idle_expires_at < now() - greatest(older_than, interval '1 day')
    RETURNING 1
  )
  SELECT (SELECT count(*) FROM spa) + (SELECT count(*) FROM operator)
$$;
