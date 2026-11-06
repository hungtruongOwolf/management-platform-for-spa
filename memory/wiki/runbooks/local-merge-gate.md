---
title: Runbook — local merge gate (khi GitHub Actions tắt)
updated: 2026-11-06
sources: []
---

# Local merge gate

GitHub Actions đang tắt vì hết phút miễn phí (log 2026-11-05). Đường merge duy nhất là `scripts/merge-local-gate.sh`: dựng cây "main + PR", chạy các bước kiểm tra trên đó, rồi `gh pr merge --squash` và xóa branch. Từ 2026-11-06 gate **theo thay đổi**: chỉ chạy những bước mà các file của PR có thể làm hỏng. Quy tắc nằm trong `scripts/gate-lib.sh`, test trong `scripts/gate-lib.test.sh` (`bash scripts/gate-lib.test.sh`, không cần build).

## Cách dùng
```bash
scripts/merge-local-gate.sh --dry-run 47        # chỉ in kế hoạch, không build, không merge
scripts/merge-local-gate.sh 47                  # gate theo thay đổi rồi merge
scripts/merge-local-gate.sh --full 47           # mọi bước + full e2e (bắt buộc trước deploy)
scripts/merge-local-gate.sh --batch 47 48       # một gate cho main + #47 + #48, rồi merge lần lượt
scripts/merge-local-gate.sh --no-e2e 47         # bỏ e2e (hoặc --e2e: luôn full e2e)
```
Env: `GATE_WORK_BASE` (mặc định `$TMPDIR/vpyd-gate`), `CARGO_BUILD_JOBS` (mặc định 2, máy 15,7 GB RAM thường chỉ trống ~4 GB), `GATE_CARGO_INCREMENTAL` (mặc định 1), `GATE_E2E_API_PORT` / `GATE_E2E_WEB_PORT` (8191 / 5291).

## Phân loại file → bước chạy
Mỗi file trong `git diff` giữa `origin/main` và cây đã merge được gắn một nhãn; kế hoạch là hợp của các nhãn.

| Nhãn | File | Kéo theo |
|---|---|---|
| `docs` | `memory/**`, `docs/**`, `*.md`, `prototypes/**`, `.claude/**`, `scripts/brand/**`, `.gitignore`… | chỉ lint |
| `rust:<crate>` | `apps/api/crates/<crate>/**` | fmt + clippy + `cargo test -p` crate đó và mọi crate phụ thuộc ngược (đồ thị path-dependency đọc từ các `Cargo.toml`, gồm dev-deps) |
| `rust:all` | `migrations/`, crate `kernel` hoặc `app`, `Cargo.toml`/`Cargo.lock` (kèm cargo-deny), toolchain, file lạ trong `apps/api` | `cargo test --workspace` |
| `sqlx` | `apps/api/.sqlx/**` | không nới rộng nếu có crate đổi cùng; đứng một mình → workspace |
| `spec` | `apps/api/openapi.json`, `scripts/api-spec.mjs`, `contract-files.mjs`, `check-extra.mjs` | sinh lại contract từ code (cần binary `app`) + full e2e |
| `client` | `packages/api-client/**` | `pnpm api:types` từ `openapi.json` đã commit rồi diff `schema.ts` (không build Rust) |
| `web:<feature>` | `apps/web/src/features/<feature>/**`; `web:_static` = `public/`, `index.html` | typecheck, `vitest --changed`, e2e chọn lọc |
| `webtest` | mọi `*.test.ts(x)` trong `apps/web/src` | typecheck, `vitest --changed`, không e2e |
| `e2e:<spec>` | `apps/web/e2e/specs/<spec>.spec.ts` | chạy spec đó |
| `shared` | phần còn lại của `apps/web` (app shell, router, `components/`, `lib/`, css, `e2e/support`, `e2e/setup`, `stack.ts`, config vite/playwright, `package.json`), `packages/ui/**`, `package.json`/`pnpm-lock.yaml` gốc, tsconfig/eslint | full vitest + `@vpyd/ui` test + full e2e |
| `gate` | chính các script gate | self-test của gate |
| `infra` | `.github/**`, Dockerfile, `vercel.json`… | không có gì chạy được; plan in ghi chú "not exercised" |
| `full` | `infra/postgres-init/**`, `infra/docker-compose.dev.yml`, mọi đường dẫn không khớp quy tắc nào | **mọi bước** (fail safe) |

Luôn chạy: kiểm tra `openapi.json` / `schema.ts` không rỗng và parse được (trước mọi build), `pnpm install`, lint. Có Rust thay đổi thì contract luôn sinh lại từ code. Plan in ra trước khi chạy, kèm danh sách file đã làm nới rộng plan.

**E2E (mặc định bật khi code app đổi).** Chọn lọc khi chỉ có `web:<feature>` / `e2e:<spec>`: setup + spec theo bảng `gate_feature_specs` (ví dụ `customers` → customers, management, sales, voids) + `layout` + `a11y` (hai spec này quét mọi màn). Full suite khi có `shared`, Rust, contract, migration hoặc feature chưa có trong bảng. Thêm feature mới thì thêm một dòng vào `gate_feature_specs`.

Không dùng `playwright --only-changed`: nó chỉ chọn spec theo import graph của chính file spec, mà spec e2e không import code app, nên sửa một màn sẽ không chọn spec nào. Bảng feature → spec thay cho nó. `vitest --changed <base>` thì đúng (unit test import code app); Vitest 5.1, Playwright 1.64.

## Build Rust một lần
- **Một worktree cố định** `$work_base/wt`, checkout tại chỗ (`checkout -f` + `clean -ffdx -e node_modules`) thay vì tạo worktree mới `pr-<n>` mỗi lần. Cây merge được dựng bằng `git merge-tree --write-tree` + `commit-tree` (không đụng worktree), nên chỉ file thật sự khác bị ghi lại: cargo so mtime nguồn + đường dẫn tuyệt đối, nên crate không đổi không build lại. Worktree cũ mỗi lần một đường dẫn mới, mtime mới → cargo build lại mọi crate trong workspace mỗi PR.
- **`CARGO_INCREMENTAL=1`** (trước là 0): với đường dẫn cố định, cache incremental của crate đổi (nhất là `app` và ~20 test binary của nó) dùng lại được. Lần đầu sau khi đổi sẽ build lại workspace một lần; tốn thêm đĩa trên C:. Muốn quay lại: `GATE_CARGO_INCREMENTAL=0`.
- **Một binary cho contract + e2e:** `kernel/testing` là dev-dependency feature, nên `cargo test` build cả đồ thị với feature đó còn `cargo run -p app` build lại toàn bộ crate workspace không có nó. Gate dùng luôn `target/debug/app(.exe)` mà `cargo test` (luôn gồm `app`, vì `app` phụ thuộc mọi crate) vừa build: `API_SPEC_BIN=… node scripts/api-spec.mjs` và `E2E_API_BIN` cho Playwright (`playwright.config.ts`, `e2e/setup/provision.setup.ts` đọc biến này). Không đổi Rust mà cần binary (e2e) thì `cargo build -p app` một lần trên target dir cố định.
- E2E của gate có DB riêng `vpyd_gate_e2e` và port riêng 8191/5291, không dùng nhầm server e2e của agent khác; sqlx test vẫn dùng `vpyd_gate`.

## Batch
`--batch a b …`: cây = main ⊕ a ⊕ b (merge-tree lần lượt; xung đột thì dừng và in file xung đột trước khi build). Một gate trên hợp, rồi merge theo thứ tự. An toàn vì trước mỗi squash gate kiểm: main chưa bị ai khác đẩy (lần đầu đúng commit đã test; các lần sau parent của main = main trước đó và tree của main = tree đã test cho bước đó), `gh pr merge --match-head-commit` (PR không có commit mới), và GitHub báo `MERGEABLE` (chờ tối đa ~2 phút khi `UNKNOWN`). Lệch bất kỳ điều kiện nào thì dừng và in các PR đã merge; không bao giờ force-push.

## Khi nào tin gate chọn lọc
- Merge PR bình thường: tin được, với điều kiện quy tắc ở trên. Rủi ro còn lại: lỗi chỉ lộ ra khi kết hợp với màn khác mà spec chọn lọc không đi qua (layout + a11y giảm bớt), và các bước gate không bao giờ chạy (Docker build, gitleaks, Lighthouse).
- **Bắt buộc `--full`** trước mọi deploy staging / Vercel ([vercel-staging](vercel-staging.md), [staging-setup](staging-setup.md)) và trước khi bật lại GitHub Actions. Có thể chạy `--full` trên PR cuối cùng trước deploy.
- Nghi ngờ, hoặc PR đụng nhiều vùng: `--full`.

## Thời gian
Cuối mỗi lần chạy gate in thời gian từng bước + tổng, và ghi một dòng vào `$work_base/gate-times.log` (PR, kết quả, plan, từng bước). Dùng log này để đo lại các ước lượng (chưa có số đo thật cho bản mới).

## Contract rỗng (PR #44, #45)
`api:spec` cũ là `cargo run … > openapi.json`: shell cắt file về rỗng trước khi cargo chạy xong, agent ngắt giữa chừng → commit `openapi.json` rỗng. Nay `pnpm api:spec` = `node scripts/api-spec.mjs`: lấy stdout, kiểm JSON có `openapi` + `paths`, ghi file tạm rồi rename; lỗi thì giữ nguyên file cũ. `scripts/contract-files.mjs` kiểm cả hai file (rỗng, không parse được, `schema.ts` thiếu `paths`/`components`/`operations`); gate chạy nó đầu tiên, `check:extra` cũng vậy.

## Bật lại CI
`gh workflow enable CI` và `gh workflow enable "Deploy (staging)"`, quay về `merge-when-green.sh`. Gate local vẫn dùng được cho batch.
