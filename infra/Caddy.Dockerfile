# syntax=docker/dockerfile:1.7
# Web image: production SPA build served by Caddy (TLS, security headers, /api proxy).
# Build context: repository root
#   docker build -f infra/Caddy.Dockerfile -t vpyd-web .

# --- Build the SPA (pnpm workspace; version pinned by package.json "packageManager").
FROM node:22-alpine AS web
ENV PNPM_HOME=/pnpm \
    CI=true
ENV PATH=$PNPM_HOME:$PATH
RUN corepack enable
WORKDIR /repo
# Lockfile first: the dependency store layer is reused until pnpm-lock.yaml changes.
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml .npmrc ./
RUN pnpm fetch --frozen-lockfile
COPY . .
RUN pnpm install --offline --frozen-lockfile \
 && pnpm --filter web build

# --- Runtime: Caddy serves the static files; no Node in the final image.
FROM caddy:2.10-alpine AS runtime
COPY infra/Caddyfile /etc/caddy/Caddyfile
COPY --from=web /repo/apps/web/dist /srv/web
# Fail the build on a broken Caddyfile (placeholder values only for validation).
RUN DOMAIN=localhost ACME_EMAIL=build@example.invalid caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile
EXPOSE 80 443 443/udp
