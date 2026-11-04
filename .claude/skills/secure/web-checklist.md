# Web application security checklist

Grounded in OWASP Top 10 and OWASP ASVS. Each item: satisfied, N/A with reason, or a finding.

## Input & output
- [ ] All input parsed into typed values at the boundary (`serde` + `validator` in Rust, `zod` in TS); unknown fields rejected; size limits on bodies, strings, arrays.
- [ ] SQL only through bound parameters (`sqlx::query!`); no string-built SQL, including `ORDER BY` — map sort keys through an allowlist.
- [ ] XSS: React escapes by default; `dangerouslySetInnerHTML` only with DOMPurify-sanitized content; URLs from users checked for `javascript:`/`data:` schemes.
- [ ] Error responses are generic (RFC 9457 problem+json); stack traces, SQL errors, and internal IDs never reach the client.

## Headers & browser
- [ ] Strict, nonce-based `Content-Security-Policy` (no `unsafe-inline` scripts); `frame-ancestors 'none'` unless embedding is a feature.
- [ ] `Strict-Transport-Security` (with preload when stable), `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin`, `Permissions-Policy` minimal.
- [ ] CORS: explicit origin allowlist; `Access-Control-Allow-Credentials` only with exact origins, never reflecting arbitrary `Origin`.
- [ ] Cookies per [authn-authz.md](authn-authz.md).

## Abuse & availability
- [ ] Rate limiting at edge (CDN/WAF) and in-app (`tower_governor` or Redis token bucket) on auth, search, expensive endpoints, media/licence endpoints.
- [ ] Request timeouts, body size limits, pagination caps (max page size enforced server-side).
- [ ] Bot/scraping defence on public content listings where content has value (rate limits, behavioural signals, Turnstile/CAPTCHA at thresholds).

## Files & outbound requests
- [ ] Uploads: size limit, type verified by content sniffing (not extension/MIME header), random server-side names, stored in a private bucket outside the web root, served via signed URLs, images re-encoded (strips EXIF/GPS and payloads), malware scan for documents.
- [ ] SSRF: any server-side fetch of a user-supplied URL goes through an allowlist; private/link-local/metadata IP ranges blocked after DNS resolution.

## Secrets & supply chain
- [ ] Secrets from environment/secret manager; `.env*` git-ignored; `gitleaks` in CI and pre-commit.
- [ ] `cargo audit` + `cargo deny`, `pnpm audit` (or osv-scanner) in CI; lockfiles committed; Dependabot/Renovate on.
- [ ] Least-privilege DB user for the app (no superuser, no DDL at runtime); migrations run with a separate role.

## Data protection (Vietnam)
- [ ] Health and medical information is **sensitive personal data** under Vietnamese law (Luật Bảo vệ dữ liệu cá nhân 91/2025/QH15 + Nghị định 356/2025/NĐ-CP, effective 01/01/2026, replacing Nghị định 13/2023). Requires explicit consent, purpose limitation, impact assessment dossier, breach notification.
- [ ] Storing or processing personal data on infrastructure outside Vietnam (any foreign cloud region) is a cross-border transfer: a transfer impact assessment is filed within 60 days. See `memory/wiki/research/hosting-storage.md`.
- [ ] Sensitive fields encrypted at rest (column-level for health notes), TLS everywhere, backups encrypted and restore-tested.
- [ ] Data minimisation: collect only what a feature needs; retention periods defined; user can export/delete.

## Logging & monitoring
- [ ] Audit log for auth events, permission changes, admin actions, access to sensitive records (who, what, when, from where).
- [ ] Logs never contain passwords, tokens, session IDs, full card numbers, or health details.
- [ ] Alerts on auth-failure spikes, 403/404 spikes from one actor (IDOR probing), unusual media download volume.
