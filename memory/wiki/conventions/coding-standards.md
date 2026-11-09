---
title: Coding standards
updated: 2026-11-09
sources: [runbooks/local-merge-gate.md, conventions/testing.md]
---

# Coding standards

Single source of truth for code quality. `build` and `pr-review` skills apply every rule here. Change a rule here, not in the skills.

## Universal
- **Names reveal intent.** A reader understands a function from its name and signature without opening it.
- **Deep modules**: small interface, substantial behaviour behind it. Split by responsibility, not by line count.
- **Make illegal states unrepresentable** with types (enums/sum types, newtypes like `UserId`, `Email`, `Money`).
- **Validate at the boundary, trust inside.** Parse untrusted input into typed domain values once, at the edge (HTTP handler, queue consumer, form).
- **Errors are values.** Handle or propagate every error explicitly; user-facing messages are generic, logs are detailed (without secrets/PII).
- **Comments explain why**, never restate what the code does.
- Delete dead code; git remembers it.
- Config from environment, validated at startup; the app refuses to boot with invalid config.
- Every behaviour change ships with a test; every bug fix ships with a test that was red before the fix.

## Rust (backend)
- `cargo fmt` + `cargo clippy --all-targets -- -D warnings` clean.
- Layering (per crate: `http.rs` → `service.rs` → `repo.rs`, ADR-0006): handlers (HTTP only) → services (business rules, authorization) → repositories (SQL). Handlers hold no business logic; repositories hold no authorization decisions.
- Errors: `thiserror` for typed domain/app errors, mapped to HTTP in one `IntoResponse` impl. `anyhow` only in binaries/scripts.
- Production paths use `?` and typed errors; `unwrap`/`expect` only in tests or with a comment proving it cannot fail.
- SQL through `sqlx` with bound parameters and compile-time checked queries (`query!`/`query_as!`). After adding or changing a query run `pnpm db:prepare` and commit `apps/api/.sqlx` (CI builds with `SQLX_OFFLINE=true`).
- Every use case runs in one `kernel::TenantTx`; repository functions take `&mut TenantTx` and still filter by `tx.tenant()` explicitly (RLS is the second layer, not the only one).
- Integration tests use `#[sqlx::test(migrator = "kernel::MIGRATOR")]` (a few older files pass `migrations = "../../migrations"`, same set) and `kernel::testing::app_pool` so they run as `app_rw` with RLS, exactly like production. Security-critical tests are mutation-checked once (break the guard, see red).
- No `unsafe`: `[workspace.lints.rust] unsafe_code = "forbid"` in `apps/api/Cargo.toml`, every crate opts in with `[lints] workspace = true`.
- Async: never block the runtime (`spawn_blocking` for CPU-heavy work such as hashing).
- Structured logging via `tracing`; secrets wrapped in `secrecy::SecretString`.

## TypeScript (frontend)
- `strict: true`, `noUncheckedIndexedAccess: true`. `any` is banned; use `unknown` + narrowing.
- API response types come from the generated contract (`@vpyd/api-client`, `pnpm api:gen` after any endpoint/DTO change; CI diffs both files). Validate other untrusted input (forms, URL params, storage) with `zod`.
- Server state via TanStack Query (the web app is a Vite SPA with TanStack Router); client state minimal and local.
- Components: one responsibility, props typed, every async UI has loading / empty / error / success states.
- Styling through design tokens only; raw hex colors and magic pixel values live in the token layer.
- ESLint + Prettier + `tsc --noEmit` clean.
- Accessibility: semantic HTML first, keyboard reachable, visible focus, `prefers-reduced-motion` respected.

## Git
- Branch per slice: `feat/<slug>`, `fix/<slug>`, `chore/<slug>`.
- Conventional Commits: `feat(api): add booking endpoint`.
- Small PRs (one slice), description links the spec/plan page in `memory/wiki/`.
- Trước khi push: `pnpm check` (= `check:api` fmt + clippy + `cargo test`, rồi lint, typecheck, unit test, `check:extra`: file contract hợp lệ, OpenAPI/TS khớp code, cargo-deny nếu đã cài).
- **Merge chỉ qua `bash scripts/merge-when-green.sh <pr>`**: chờ check xuất hiện, chờ chạy xong, chỉ merge khi mọi check `pass`/`skipping` và có `CI ok: pass`. Không gọi `gh pr merge` trực tiếp (PR #8 từng bị merge trước khi CI chạy). Khi GitHub Actions không chạy được: `scripts/merge-local-gate.sh` ([local-merge-gate](../runbooks/local-merge-gate.md)).
- Không force-push (bị chặn). Cần thay lịch sử sau rebase → push sang branch mới `…-v2`, mở PR mới, đóng PR cũ.
