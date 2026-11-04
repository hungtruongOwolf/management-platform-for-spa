#!/bin/sh
# Restore a backup made by backup.sh into an EMPTY database (restore drill, or rebuilding after a disaster).
#   restore.sh latest                      newest object under $BACKUP_PREFIX/daily/
#   restore.sh staging/daily/<file>.age    a specific object (path inside the bucket / BACKUP_REMOTE)
#
# Target database (RESTORE_* so a restore can never silently reuse the backup source credentials):
#   RESTORE_PGHOST RESTORE_PGPORT RESTORE_PGUSER RESTORE_PGPASSWORD RESTORE_PGDATABASE [RESTORE_PGSSLMODE]
# Decryption:
#   AGE_IDENTITY_FILE        age private key file, mounted read-only only for the duration of the drill
# Source: same R2_* / BACKUP_PREFIX / BACKUP_REMOTE variables as backup.sh.
# Prerequisite on the target: cluster role app_rw exists (infra/postgres-init/01-roles.sql), so GRANTs restore.
set -eu
set -o pipefail

log() { printf '%s restore: %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*" >&2; }
die() { log "FAILED: $*"; exit 1; }

[ $# -eq 1 ] || die "usage: restore.sh latest | <object path>"
for name in RESTORE_PGHOST RESTORE_PGUSER RESTORE_PGDATABASE AGE_IDENTITY_FILE; do
  eval "value=\${$name:-}"
  [ -n "$value" ] || die "$name is required"
done
[ -r "$AGE_IDENTITY_FILE" ] || die "AGE_IDENTITY_FILE is not readable"

if [ -z "${BACKUP_REMOTE:-}" ]; then
  for name in R2_ACCOUNT_ID R2_ACCESS_KEY_ID R2_SECRET_ACCESS_KEY R2_BUCKET; do
    eval "value=\${$name:-}"
    [ -n "$value" ] || die "$name is required"
  done
  export RCLONE_CONFIG_R2_TYPE=s3
  export RCLONE_CONFIG_R2_PROVIDER=Cloudflare
  export RCLONE_CONFIG_R2_ACCESS_KEY_ID="$R2_ACCESS_KEY_ID"
  export RCLONE_CONFIG_R2_SECRET_ACCESS_KEY="$R2_SECRET_ACCESS_KEY"
  export RCLONE_CONFIG_R2_ENDPOINT="https://${R2_ACCOUNT_ID}.r2.cloudflarestorage.com"
  export RCLONE_CONFIG_R2_REGION=auto
  BACKUP_REMOTE="r2:${R2_BUCKET}"
fi
PREFIX="${BACKUP_PREFIX:-staging}"

# Point libpq at the restore target only; drop any inherited source variables.
unset PGHOST PGPORT PGUSER PGPASSWORD PGDATABASE PGSSLMODE PGSSLROOTCERT
export PGHOST="$RESTORE_PGHOST" PGPORT="${RESTORE_PGPORT:-5432}" PGUSER="$RESTORE_PGUSER" \
       PGDATABASE="$RESTORE_PGDATABASE" PGSSLMODE="${RESTORE_PGSSLMODE:-prefer}"
[ -z "${RESTORE_PGPASSWORD:-}" ] || export PGPASSWORD="$RESTORE_PGPASSWORD"

if [ "$1" = "latest" ]; then
  object="$(rclone lsf --files-only "${BACKUP_REMOTE}/${PREFIX}/daily/" | sort | tail -n 1)"
  [ -n "$object" ] || die "no backups under ${PREFIX}/daily/"
  object="${PREFIX}/daily/${object}"
else
  object="$1"
fi
log "restoring ${object} into ${PGDATABASE} on ${PGHOST}"

# Safety: refuse to restore over existing application data.
existing="$(psql -X -At -v ON_ERROR_STOP=1 -c "SELECT count(*) FROM pg_namespace WHERE nspname = 'app'")" \
  || die "cannot connect to the restore target"
[ "$existing" = "0" ] || die "target already has schema app; restore only into an empty database"
psql -X -At -v ON_ERROR_STOP=1 -c "SELECT 1 FROM pg_roles WHERE rolname = 'app_rw'" | grep -q 1 \
  || die "role app_rw is missing on the target (run infra/postgres-init/01-roles.sql with a real password first)"

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT INT TERM
# The decrypted dump lives only in this container's tmpfs-backed workdir and is removed on exit.
rclone cat "${BACKUP_REMOTE}/${object}" | age --decrypt --identity "$AGE_IDENTITY_FILE" > "$workdir/db.dump" \
  || die "download/decrypt failed"

# The public schema already exists on every target: skip its CREATE/COMMENT entries, restore everything else.
pg_restore --list "$workdir/db.dump" | grep -v -E ' SCHEMA - public | COMMENT - SCHEMA public ' > "$workdir/toc.list"
# Extensions are not part of a schema-scoped dump; create the ones the migrations use first.
for ext in ${RESTORE_EXTENSIONS:-pg_trgm}; do
  psql -X -v ON_ERROR_STOP=1 -q -c "CREATE EXTENSION IF NOT EXISTS ${ext}" || die "cannot create extension ${ext}"
done
pg_restore --no-owner --exit-on-error --single-transaction --use-list="$workdir/toc.list" \
  --dbname="$PGDATABASE" "$workdir/db.dump" || die "pg_restore failed"

psql -X -q -c "ANALYZE" >/dev/null 2>&1 || true
log "verification:"
psql -X -v ON_ERROR_STOP=1 <<'SQL'
\pset footer off
SELECT max(version) AS last_migration, count(*) AS migrations FROM public._sqlx_migrations;
SELECT relname AS table, n_live_tup AS approx_rows FROM pg_stat_user_tables WHERE schemaname = 'app' ORDER BY relname;
SELECT count(*) AS tables_without_rls FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
 WHERE n.nspname = 'app' AND c.relkind = 'r' AND NOT c.relrowsecurity;
SQL
log "done: compare row counts with production before signing off the drill"
