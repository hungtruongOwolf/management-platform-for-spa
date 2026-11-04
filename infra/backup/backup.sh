#!/bin/sh
# Nightly logical backup: pg_dump (custom format) -> age encryption (public key) -> Cloudflare R2 (S3 API).
# Streams end to end: no plaintext dump ever touches disk. Secrets come from the environment only, never argv.
#
# Database (libpq variables, so the password never appears in a command line):
#   PGHOST PGPORT PGUSER PGPASSWORD PGDATABASE PGSSLMODE [PGSSLROOTCERT]
# Encryption:
#   AGE_RECIPIENT            age public key (age1...). The private key is kept offline, never on the server.
# Destination (Cloudflare R2):
#   R2_ACCOUNT_ID R2_ACCESS_KEY_ID R2_SECRET_ACCESS_KEY R2_BUCKET
#   BACKUP_PREFIX            folder inside the bucket, e.g. "staging" (default)
#   BACKUP_REMOTE            override the rclone destination (e.g. a local path for tests); default "r2:$R2_BUCKET"
# Retention (optional; prefer R2 lifecycle rules + bucket lock, see runbook):
#   BACKUP_RETENTION_DAYS    delete daily/ objects older than N days
#   BACKUP_MONTHLY_RETENTION_DAYS  delete monthly/ objects older than N days
# Monitoring (optional):
#   HEALTHCHECK_PING_URL     dead-man's switch (healthchecks.io); "/fail" is appended on failure
set -eu
set -o pipefail

log() { printf '%s backup: %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*" >&2; }

ping() {
  [ -n "${HEALTHCHECK_PING_URL:-}" ] || return 0
  curl -fsS -m 10 --retry 3 -o /dev/null "${HEALTHCHECK_PING_URL}$1" || log "warning: healthcheck ping failed"
}

fail() {
  log "FAILED: $*"
  ping /fail
  exit 1
}

require() {
  for name in "$@"; do
    eval "value=\${$name:-}"
    [ -n "$value" ] || fail "$name is required"
  done
}

require PGHOST PGUSER PGPASSWORD PGDATABASE AGE_RECIPIENT
case "$AGE_RECIPIENT" in age1*) ;; *) fail "AGE_RECIPIENT must be an age public key (age1...)" ;; esac

if [ -z "${BACKUP_REMOTE:-}" ]; then
  require R2_ACCOUNT_ID R2_ACCESS_KEY_ID R2_SECRET_ACCESS_KEY R2_BUCKET
  # rclone remote "r2" configured from the environment (no config file holding credentials).
  export RCLONE_CONFIG_R2_TYPE=s3
  export RCLONE_CONFIG_R2_PROVIDER=Cloudflare
  export RCLONE_CONFIG_R2_ACCESS_KEY_ID="$R2_ACCESS_KEY_ID"
  export RCLONE_CONFIG_R2_SECRET_ACCESS_KEY="$R2_SECRET_ACCESS_KEY"
  export RCLONE_CONFIG_R2_ENDPOINT="https://${R2_ACCOUNT_ID}.r2.cloudflarestorage.com"
  export RCLONE_CONFIG_R2_REGION=auto
  export RCLONE_CONFIG_R2_NO_CHECK_BUCKET=true
  BACKUP_REMOTE="r2:${R2_BUCKET}"
fi
PREFIX="${BACKUP_PREFIX:-staging}"
export PGSSLMODE="${PGSSLMODE:-verify-full}"
export PGCONNECT_TIMEOUT="${PGCONNECT_TIMEOUT:-15}"

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
name="vpyd-${PREFIX}-${stamp}.dump.age"
daily="${BACKUP_REMOTE}/${PREFIX}/daily/${name}"

ping /start
log "dumping ${PGDATABASE} on ${PGHOST} -> ${PREFIX}/daily/${name}"

# Scope: the application schema plus public (sqlx's _sqlx_migrations). Supabase-internal schemas
# (auth, storage, realtime, ...) are not ours and are excluded by not being selected.
if ! pg_dump --format=custom --compress=6 --no-password --schema=app --schema=public \
     | age --encrypt --recipient "$AGE_RECIPIENT" \
     | rclone rcat --s3-no-check-bucket "$daily"; then
  rclone deletefile "$daily" >/dev/null 2>&1 || true   # never leave a truncated backup that looks valid
  fail "dump/encrypt/upload pipeline failed"
fi

size="$(rclone size --json "$daily" | sed -n 's/.*"bytes":\([0-9]*\).*/\1/p')"
[ "${size:-0}" -gt 1024 ] || fail "uploaded object is suspiciously small (${size:-0} bytes)"
log "uploaded ${size} bytes"

# First backup of each month is also kept as a monthly copy (longer retention).
if [ "$(date -u +%d)" = "01" ]; then
  rclone copyto "$daily" "${BACKUP_REMOTE}/${PREFIX}/monthly/${name}" || fail "monthly copy failed"
  log "monthly copy written"
fi

if [ -n "${BACKUP_RETENTION_DAYS:-}" ]; then
  rclone delete --min-age "${BACKUP_RETENTION_DAYS}d" "${BACKUP_REMOTE}/${PREFIX}/daily/" || log "warning: daily prune failed"
fi
if [ -n "${BACKUP_MONTHLY_RETENTION_DAYS:-}" ]; then
  rclone delete --min-age "${BACKUP_MONTHLY_RETENTION_DAYS}d" "${BACKUP_REMOTE}/${PREFIX}/monthly/" || log "warning: monthly prune failed"
fi

ping ""
log "done"
