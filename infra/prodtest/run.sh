#!/bin/sh
# Local prod-like smoke test of the staging stack (S11). Uses its own compose project and host ports
# 18080/18443, never touches the dev Postgres. Throwaway secrets are generated into .prodtest/ (git-ignored).
#   sh infra/prodtest/run.sh          build, start, check, back up, restore drill, tear down
#   KEEP=1 sh infra/prodtest/run.sh   leave the stack running (tear down: see the last line it prints)
set -eu

root="$(cd "$(dirname "$0")/../.." && pwd)"
work="$root/.prodtest"
project=vpyd-prodtest
host_path() { if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else printf '%s' "$1"; fi; }
compose() {
  docker compose -p "$project" --env-file "$work/env" \
    -f "$root/infra/docker-compose.prod.yml" -f "$root/infra/prodtest/docker-compose.prodtest.yml" "$@"
}
check() { printf '  %-52s' "$1"; shift; if "$@" >/dev/null 2>&1; then echo ok; else echo FAIL; exit 1; fi; }
cleanup() { [ "${KEEP:-}" = 1 ] || compose --profile ops --profile drill down -v --remove-orphans >/dev/null 2>&1 || true; }

echo "== build images"
docker build -q -t vpyd-api:prodtest "$root/apps/api"
docker build -q -f "$root/infra/Caddy.Dockerfile" -t vpyd-web:prodtest "$root"
docker build -q -t vpyd-backup:prodtest "$root/infra/backup"
docker image ls --format '  {{.Repository}}:{{.Tag}} {{.Size}}' | grep prodtest

echo "== throwaway secrets"
rm -rf "$work"; mkdir -p "$work/secrets"
: > "$work/secrets/supabase-ca.crt"                       # unused: the stand-in DB has no TLS
docker run --rm --entrypoint age-keygen vpyd-backup:prodtest > "$work/secrets/age-identity.txt" 2>/dev/null
recipient="$(sed -n 's/^# public key: //p' "$work/secrets/age-identity.txt")"
pw="$(openssl rand -hex 16)"
cat > "$work/env" <<EOF
API_IMAGE=vpyd-api:prodtest
WEB_IMAGE=vpyd-web:prodtest
BACKUP_IMAGE=vpyd-backup:prodtest
DOMAIN=localhost
ACME_EMAIL=prodtest@example.invalid
APP_ENV=staging
APP_PUBLIC_ORIGIN=https://localhost:18443
HTTP_PORT=18080
HTTPS_PORT=18443
SECRETS_DIR=$(host_path "$work/secrets")
FAKEDB_PASSWORD=$pw
MIGRATE_DATABASE_URL=postgres://postgres:$pw@fakedb:5432/vpyd?sslmode=disable
APP_DATABASE_URL=postgres://app_rw:app_rw_dev_password@fakedb:5432/vpyd?sslmode=disable
APP_DATA_KEY=$(openssl rand -base64 32)
BACKUP_PGHOST=fakedb
BACKUP_PGUSER=postgres
BACKUP_PGPASSWORD=$pw
BACKUP_PGDATABASE=vpyd
BACKUP_PGSSLMODE=disable
AGE_RECIPIENT=$recipient
R2_ACCOUNT_ID=unused
R2_ACCESS_KEY_ID=unused
R2_SECRET_ACCESS_KEY=unused
R2_BUCKET=unused
BACKUP_REMOTE=/work/out
EOF
trap cleanup EXIT

echo "== start stack (migrate -> api healthy -> caddy)"
compose up -d --wait fakedb
check "migrate (one-shot, owner role) exits 0" compose run --rm migrate
compose up -d --wait --no-deps api caddy
compose ps -a --format '  {{.Service}}: {{.Status}}'
check "api container healthy (busybox wget)" sh -c "docker inspect -f '{{.State.Health.Status}}' ${project}-api-1 | grep -qx healthy"

base=https://localhost:18443
echo "== checks via Caddy (internal CA, so -k)"
check "GET /api/v1/health -> 200 ok" sh -c "curl -ksf $base/api/v1/health | grep -q '\"status\":\"ok\"'"
check "GET / -> SPA index.html" sh -c "curl -ksf $base/ | grep -q '<div id=\"root\">'"
check "GET /bang-gia (deep link) -> SPA fallback" sh -c "curl -ksf $base/bang-gia | grep -q '<div id=\"root\">'"
check "missing /assets/* -> 404 (no fallback)" sh -c "test \"\$(curl -ks -o /dev/null -w '%{http_code}' $base/assets/nope.js)\" = 404"
check "CSP header strict" sh -c "curl -ksI $base/ | grep -qi \"content-security-policy: default-src 'self'; script-src 'self';\""
check "HSTS header" sh -c "curl -ksI $base/ | grep -qi 'strict-transport-security: max-age=31536000'"
check "no Server header" sh -c "! curl -ksI $base/ | grep -qi '^server:'"
check "index.html no-cache" sh -c "curl -ksI $base/ | grep -qi 'cache-control: no-cache'"
asset="$(curl -ksf $base/ | sed -n 's/.*src="\(\/assets\/[^"]*\.js\)".*/\1/p' | head -n 1)"
check "hashed asset immutable ($asset)" sh -c "curl -ksI $base$asset | grep -qi 'immutable'"
check "http -> https redirect" sh -c "curl -s -o /dev/null -w '%{http_code}' http://localhost:18080/ | grep -qE '30[18]'"
curl -ks -o /dev/null "$base/api/v1/customers?q=0905111222" || true
check "access log drops query strings (L5)" sh -c "docker logs ${project}-caddy-1 2>&1 | grep -q 'customers?\[redacted\]' && ! docker logs ${project}-caddy-1 2>&1 | grep -q 0905111222"
check "API has no host port (only via Caddy)" sh -c "test -z \"\$(docker port ${project}-api-1)\""

echo "== backup -> restore drill"
compose --profile ops run --rm backup
compose --profile drill up -d --wait restore-db
compose --profile drill run --rm restore latest

if [ "${KEEP:-}" = 1 ]; then
  echo "stack kept. tear down: docker compose -p $project --env-file .prodtest/env -f infra/docker-compose.prod.yml -f infra/prodtest/docker-compose.prodtest.yml --profile ops --profile drill down -v"
fi
echo "== all checks passed"
