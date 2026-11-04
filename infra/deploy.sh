#!/bin/sh
# Server-side deploy / rollback. Runs on the VPS as user `deploy` from /opt/vpyd.
#   infra/deploy.sh <tag>                deploy images ghcr.io/<owner>/vpyd-{api,web,backup}:<tag>
#   infra/deploy.sh --rollback           redeploy the previous tag from .deploy-history
# Order: pull -> migrate (one-shot, owner role) -> api healthy -> caddy -> public smoke check.
# Migrations must stay backward compatible with the previous API version (expand/contract), because a
# rollback redeploys the old API against the already-migrated schema.
set -eu
cd /opt/vpyd

env_file=.env.production
history=.deploy-history
registry="${REGISTRY:-ghcr.io/hungtruongowolf}"
[ -f "$env_file" ] || { echo "missing /opt/vpyd/$env_file" >&2; exit 1; }

if [ "${1:-}" = "--rollback" ]; then
  tag="$(tail -n 2 "$history" 2>/dev/null | head -n 1)"
  [ -n "$tag" ] || { echo "no previous deploy recorded" >&2; exit 1; }
else
  tag="${1:?usage: deploy.sh <tag> | --rollback}"
fi
case "$tag" in *[!A-Za-z0-9._-]*) echo "invalid tag" >&2; exit 1 ;; esac

set_var() {  # replace KEY=... in the env file without printing any other line
  if grep -q "^$1=" "$env_file"; then sed -i "s|^$1=.*|$1=$2|" "$env_file"; else printf '%s=%s\n' "$1" "$2" >> "$env_file"; fi
}
set_var API_IMAGE "$registry/vpyd-api:$tag"
set_var WEB_IMAGE "$registry/vpyd-web:$tag"
set_var BACKUP_IMAGE "$registry/vpyd-backup:$tag"

compose() { docker compose --env-file "$env_file" -f infra/docker-compose.prod.yml "$@"; }
compose pull --quiet migrate api caddy
compose --profile ops pull --quiet backup
compose run --rm migrate            # fails the deploy before any traffic moves
compose up -d --wait --no-deps --remove-orphans api caddy

domain="$(sed -n 's/^DOMAIN=\([^ #]*\).*/\1/p' "$env_file")"
if ! curl -fsS -m 10 --retry 5 --retry-delay 3 "https://$domain/api/v1/health" | grep -q '"status":"ok"'; then
  echo "smoke check failed for $tag; roll back with: infra/deploy.sh --rollback" >&2
  exit 1
fi
[ "$(tail -n 1 "$history" 2>/dev/null)" = "$tag" ] || echo "$tag" >> "$history"
docker image prune -f --filter "until=168h" >/dev/null
echo "deployed $tag"
