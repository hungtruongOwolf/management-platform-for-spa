# Authentication & Authorization checklist

## Authentication (who are you)
- [ ] Use a vetted identity solution: a managed/self-hosted OIDC provider (e.g. Zitadel, Keycloak, Ory, Auth0, Clerk) or well-reviewed libraries. Hand-rolled crypto never ships.
- [ ] Passwords: Argon2id (`argon2` crate) with per-user salt, run in `spawn_blocking`. Minimum length 10+, check against breached-password lists, no composition rules.
- [ ] Login, signup, reset, and OTP endpoints are rate-limited per IP **and** per account, with progressive delay. Responses are identical for "no such user" and "wrong password".
- [ ] Reset / magic-link / email-verify tokens: ≥128-bit random, single-use, TTL ≤ 30 min, stored hashed.
- [ ] MFA available (TOTP / passkeys); required for admin and staff roles.
- [ ] Re-authentication required for sensitive actions (change email/password, payout details, delete account).

## Sessions (web)
- [ ] Opaque session ID in a cookie: `HttpOnly; Secure; SameSite=Lax` (or `Strict`), `__Host-` prefix, `Path=/`. Session data lives server-side (Postgres/Redis).
- [ ] Session ID rotated on login and on privilege change; old ID invalidated.
- [ ] Idle timeout and absolute timeout; logout deletes the server-side session; "log out all devices" exists.
- [ ] Tokens never stored in `localStorage`/`sessionStorage` (readable by any XSS).
- [ ] CSRF: `SameSite` cookie **plus** Origin/Referer check or CSRF token on every state-changing request. GET never changes state.

## Tokens (mobile apps / service-to-service only)
- [ ] Access token TTL ≤ 15 min; refresh tokens rotate with reuse detection (reuse → revoke the family).
- [ ] JWT verification pins the algorithm, checks `iss`, `aud`, `exp`, `nbf`; `alg: none` and HS/RS confusion rejected.
- [ ] Revocation path exists (denylist or short TTL + session check).

## Authorization (what may you do)
- [ ] **Deny by default.** Every route declares its required permission; a route without a declaration fails a test.
- [ ] One central policy function — `authorize(actor, action, resource) -> Result<(), Forbidden>` — called in the **service layer**. Handlers and the frontend never decide access.
- [ ] Identity comes only from the server session (`AuthUser` extractor). `user_id`, `role`, `tenant_id`, `price`, `is_premium` in a request body/query/header are ignored or rejected.
- [ ] **Ownership on every resource access** (IDOR defence): queries are scoped, e.g. `WHERE id = $1 AND owner_id = $2`, never "fetch by id, then maybe check". Prefer non-guessable IDs (UUIDv7) as defence in depth — never as the only defence.
- [ ] Mass assignment blocked: request DTOs list exactly the writable fields; `role`, `status`, `owner_id`, `price` are never deserialized from user input.
- [ ] Business-flow integrity: prices, discounts, totals, and entitlements are computed server-side; step order (pay → grant access) is enforced by server state, not by which page the client visits.
- [ ] Admin API lives behind separate routes with stronger auth (MFA, optional IP allowlist) and full audit logging.
- [ ] `memory/wiki/security/authz-matrix.md` maps role × action × resource, and a test suite asserts every cell — especially the **negative** cells (user A reading user B's data must return 404/403).

## Frontend reality
- Route guards, hidden buttons, disabled inputs, and role checks in React are UX. Attackers bypass them in seconds.
- Admin/staff UI code is downloadable by anyone if it ships in the public bundle: it holds no secrets, and the server re-checks every call it makes.
- Never put secrets in env vars exposed to the client (`NEXT_PUBLIC_*`, `VITE_*`).

## Rust pattern sketch
```rust
// Extractor: fails with 401 when no valid session.
pub struct AuthUser { pub id: UserId, pub roles: RoleSet }

// Service layer owns the decision; repository query is owner-scoped.
pub async fn get_record(ctx: &AppCtx, actor: &AuthUser, id: RecordId) -> Result<Record, AppError> {
    policy::authorize(actor, Action::Read, Resource::Record(id))?;
    ctx.repo.find_record_for_owner(id, actor.id).await?.ok_or(AppError::NotFound)
}
```
