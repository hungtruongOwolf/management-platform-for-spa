---
title: "Thiết kế: Vòng quay may mắn (API, DB, web)"
status: approved (lead 2026-11-27); agent build cập nhật mục "Khi build" nếu lệch
updated: 2026-11-27
sources: [../product/specs/lucky-draw.md, raw/2026-11-27-em-request-lucky-draw.md]
related: [backend.md, data.md, frontend.md, ../security/authz-matrix.md, ../design/design-system.md]
---

# Vòng quay may mắn: thiết kế

Spec: [lucky-draw](../product/specs/lucky-draw.md). Trang này là **hợp đồng** giữa backend và web; đổi shape thì sửa ở đây trước.

## 1. Module
- Crate mới **`lucky_draw`** (ADR-0006: mỗi bounded context một crate): `domain` (hàm thuần: đủ điều kiện, còn lại, xong chưa, chọn ngẫu nhiên đều từ một nguồn ngẫu nhiên tiêm vào), `service` (use case + SQL), `http` (route + OpenAPI). Đăng ký route ở `app`. RNG: `rand::rngs::OsRng` (hoặc `getrandom`) ở tầng service, test tiêm RNG cố định.
- Permission mới `lucky_draw.manage` (`kernel::permission::Permission::LuckyDrawManage`): role owner của mọi spa + mẫu vai trò `owner` (migration, như `record.history.read` ở 0052), danh mục permission của console `/admin` biết tên này (không `422 unknown_permission`).
- Audit (typed `kernel::audit::Action`, mỗi action có nhóm + câu ở web): `lucky_draw.create`, `lucky_draw.rename`, `lucky_draw.delete`, `lucky_draw.participants_add`, `lucky_draw.participant_update`, `lucky_draw.participant_delete`, `lucky_draw.prize_create`, `lucky_draw.prize_update`, `lucky_draw.prize_delete`, `lucky_draw.spin`, `lucky_draw.forfeit`. Lịch sử trên từng bản ghi (ADR-0022): cả 11 action **không** có mục lịch sử tại chỗ (`RecordTrace` → `null`); Nhật ký thao tác (owner, sau cờ) có câu tiếng Việt.

## 2. DB (migration 0055)
```
lucky_draw              (tenant_id, id uuid pk, name text 1..80, created_by → user_account, created_at, updated_at, version bigint)
lucky_draw_participant  (tenant_id, id, draw_id → lucky_draw ON DELETE CASCADE, name text 1..120,
                         gender text CHECK in ('male','female'), position int, created_by, created_at, updated_at, version)
lucky_draw_prize        (tenant_id, id, draw_id → … CASCADE, name 1..140, description ≤ 250 NULL, eligible CHECK in ('all','male','female'),
                         quantity int CHECK 1..1000, position int, created_by, created_at, updated_at, version)
lucky_draw_result       (tenant_id, id, draw_id → … CASCADE, prize_id → lucky_draw_prize, participant_id → lucky_draw_participant,
                         seq int > 0, drawn_at, drawn_by, forfeited_at NULL, forfeited_by NULL)
```
- FK luôn kèm `tenant_id` (`(tenant_id, draw_id) → lucky_draw (tenant_id, id)`), RLS `tenant_isolation` trên cả 4 bảng, `app_rw` chỉ quyền cần dùng.
- Kết quả: `UNIQUE (tenant_id, draw_id, participant_id)`, `UNIQUE (tenant_id, draw_id, seq)`, CHECK `(forfeited_at IS NULL) = (forfeited_by IS NULL)`; trigger: chỉ cho `UPDATE` đặt `forfeited_*` từ NULL một lần, mọi cột khác bất biến; prize / participant của kết quả phải cùng `draw_id` (FK ba cột `(tenant_id, draw_id, prize_id)` hoặc trigger). Prize / participant có kết quả không xóa được (FK `ON DELETE RESTRICT` từ result; xóa chương trình vẫn CASCADE vì cả cây đi cùng).
- Index: `lucky_draw (tenant_id, created_at DESC)`, con theo `(tenant_id, draw_id)`.
- Thêm vào `app.invariant_violations()` nếu rẻ: số kết quả chưa trả của một giải ≤ `quantity`; người trúng khớp giới của giải.

## 3. API (`/api/v1`, mọi route `lucky_draw.manage`, id lạ / spa khác `404`)
| Route | Thân | Trả |
|---|---|---|
| `GET /lucky-draws` | — | `200 {items: LuckyDrawSummary[]}` |
| `POST /lucky-draws` | `{name}` | `201 LuckyDrawDetail` |
| `GET /lucky-draws/{id}` | — | `200 LuckyDrawDetail` + `ETag` |
| `PUT /lucky-draws/{id}` (If-Match) | `{name}` | `200 LuckyDrawDetail` |
| `DELETE /lucky-draws/{id}` | — | `204` |
| `POST /lucky-draws/{id}/participants` | `{participants: [{name, gender}]}` 1..500 | `201 LuckyDrawDetail` |
| `PUT /lucky-draws/{id}/participants/{pid}` (If-Match = version người đó) | `{name, gender}` | `200 LuckyDrawDetail` |
| `DELETE /lucky-draws/{id}/participants/{pid}` | — | `200 LuckyDrawDetail` |
| `POST /lucky-draws/{id}/prizes` | `{name, description?, eligible, quantity}` | `201 LuckyDrawDetail` |
| `PUT /lucky-draws/{id}/prizes/{qid}` (If-Match) | như trên | `200 LuckyDrawDetail` |
| `DELETE /lucky-draws/{id}/prizes/{qid}` | — | `200 LuckyDrawDetail` |
| `POST /lucky-draws/{id}/spins` (**Idempotency-Key** bắt buộc) | `{prize_id}` | `201 SpinOutcome` |
| `POST /lucky-draws/{id}/results/{rid}/forfeit` | — | `200 LuckyDrawDetail` |

Mọi lệnh ghi trả **toàn bộ** `LuckyDrawDetail` (web thay cache một lần, không phải gọi lại `GET`). Route `POST participants` có giới hạn thân riêng 256 KB (mặc định 64 KB không đủ 500 tên dài); web chia danh sách dán thành lô ≤ 500, gửi tuần tự.

```ts
LuckyDrawSummary = { id, name, created_at, participant_count, prize_slots /* tổng quantity */, awarded /* kết quả chưa trả */, status: "not_started" | "in_progress" | "finished" }
LuckyDrawDetail  = { id, name, created_at, version, status, finished: boolean,
  participants: { id, name, gender: "male"|"female", position, version, result_id: string|null }[],   // thứ tự position
  prizes:       { id, name, description: string|null, eligible: "all"|"male"|"female", quantity, awarded, remaining,
                  eligible_count /* người còn đủ điều kiện */, position, version }[],
  results:      { id, seq, prize_id, participant_id, drawn_at, forfeited_at: string|null }[] }           // thứ tự seq
SpinOutcome = { result: { id, seq, prize_id, participant_id, drawn_at }, draw: LuckyDrawDetail }
```
- `status` / `finished` tính ở server bằng đúng một hàm (`domain`): `finished` = có ≥ 1 kết quả và không giải nào có `remaining > 0 && eligible_count > 0`.
- Lỗi (`problem+json` `code`): `validation_failed` (+ `errors[].field`: `name`, `participants[i].name`, `participants[i].gender`, `participants` (quá số lượng), `description`, `eligible`, `quantity`, `prizes` (> 50)), `lucky_draw.participant_drawn` 409, `lucky_draw.prize_drawn` 409, `lucky_draw.quantity_below_awarded` 422, `lucky_draw.prize_exhausted` 409, `lucky_draw.no_eligible` 409, `lucky_draw.result_forfeited` 409, cùng bộ `precondition.if_match_required` 400 / `conflict.stale_version` 412 / `idempotency.*` như các route khác.
- **Quay**: một transaction: `SELECT … FROM lucky_draw … FOR UPDATE` (mọi lệnh ghi của chương trình đều khóa dòng này trước) → đọc giải, `remaining`, nhóm đủ điều kiện → chọn `idx = uniform(0, n)` bằng OS RNG (không modulo bias) → `INSERT result (seq = max + 1)` → audit `lucky_draw.spin` (`after`: seq, tên giải, tên người trúng) → commit. Phản hồi idempotent lưu `{result_id}` rồi đọc lại khi replay (không lưu tên trong bản ghi idempotency).
- `version` của chương trình tăng ở **mọi** lệnh ghi trong chương trình (đổi tên, người, giải, quay, trả giải) để `ETag` của `GET` đổi theo; `PUT /lucky-draws/{id}` dùng nó cho `If-Match`.

## 4. Web
- Thư mục `apps/web/src/features/lucky-draw/`; màn lazy `LuckyDrawListPage` (`/vong-quay`), `LuckyDrawPage` (`/vong-quay/$drawId`); đăng ký ở `app/screens.ts` / router / `mayOpen` / `navFor` (nhóm Khách hàng, sau "Khách lâu chưa quay lại") / command palette. Không import gì từ màn này vào startup.
- Query key `["lucky-draws"]`, `["lucky-draws", id]`; mọi mutation `setQueryData` bằng `LuckyDrawDetail` trả về + invalidate danh sách.
- `Wheel` component: hai ảnh `wheel-disk.webp` (xoay, `border-radius: 50%`) + `wheel-frame.webp` (đứng yên) theo tỉ lệ vị trí của bản gốc (`left 8.798%`, `top 13.929%`, `width 80.687%`, `height 78.170%`, khung 466 × 481). Xoay (sửa 2026-11-28, như bản HTML của EM): nhấn Quay → ngay một transition +6 vòng tới giữa sector bốc lúc bấm (8 sector, `crypto.getRandomValues`, `sector × 45 + 22,5°`) trong 4100 ms `cubic-bezier(.14,.72,.12,1)`; reduced motion **vẫn quay**: 2 vòng / 2500 ms, ease mềm hơn. Lượt đáp khi vòng đã dừng (`transitionend` hoặc hẹn giờ) **và** đã có `SpinOutcome`; lúc đó mới đưa vào cache và mở popup người trúng (`WinnerDialog`). Ảnh copy từ bản đã nén (lead để ở scratchpad), import qua Vite.
- Hàm thuần có Vitest: parse danh sách dán (`parseRoster`), chọn giải mặc định (`defaultPrize`), gom tổng kết theo giải (`summaryByPrize`), CSV (`winnersCsv` có chống injection).
- e2e `lucky-draw.spec.ts`: chủ spa tạo chương trình → dán 6 người (3 Nữ, 3 Nam) → 2 giải (Chỉ nữ × 2, Cả hai × 2) → quay tới hết → tổng kết đúng giới → trả giải một người → quay lại → xóa chương trình (e2e không để lại dữ liệu); quản lý không thấy nav, mở URL → màn 403 của app; axe sạch.

## 5. Khi build
Backend: PR `feat/lucky-draw-api` (agent backend, 2026-11-27). Crate `apps/api/crates/lucky_draw` (`domain.rs` hàm thuần + `RandomSource`, `repo.rs` SQL, `service.rs` use case, `http.rs` route + OpenAPI), migration `0055_lucky_draw.sql`, test `apps/api/crates/app/tests/lucky_draw.rs` + 13 dòng `authz_matrix.rs`. Quyết định quay ở server: [ADR-0025](../decisions/ADR-0025-lucky-draw-server-side-draw.md).

**Lệch / chốt so với §1–§3 (web dựa vào đây):**
- **B1 — `PUT`, không `PATCH`.** Spec US1–US3 viết `PATCH`; hợp đồng §3 (ràng buộc) viết `PUT` thay toàn bộ trường sửa được. Build theo §3: `PUT /lucky-draws/{id}` `{name}`, `PUT …/participants/{pid}` `{name, gender}`, `PUT …/prizes/{qid}` `{name, description?, eligible, quantity}`, đều bắt buộc `If-Match`.
- **B2 — `SpinOutcome` theo §3**, không theo câu AC4.2 cũ `{result, prize_remaining, finished}`: `201 {result: {id, seq, prize_id, participant_id, drawn_at}, draw: LuckyDrawDetail}` (số suất còn lại và `finished` nằm trong `draw`). Mọi lệnh ghi trả kèm `ETag` = `version` của chương trình.
- **B3 — tên schema trong `@vpyd/api-client`** (`packages/api-client/src/index.ts`): `LuckyDrawSummary`, `LuckyDrawList` (`{items}`), `LuckyDrawDetail`, `LuckyDrawParticipant`, `LuckyDrawPrize`, `LuckyDrawResult`, `LuckyDrawSpinResult`, `SpinOutcome`, `LuckyDrawStatus`, `LuckyDrawGender`, `LuckyDrawEligible`; thân ghi `LuckyDrawWrite` (`{name}`), `LuckyDrawParticipantsAdd` (`{participants}`), `LuckyDrawParticipantWrite` (`{name, gender}`), `LuckyDrawPrizeWrite`, `LuckyDrawSpin` (`{prize_id}`). Trường `result_id`, `description`, `forfeited_at` luôn có mặt (`string | null`).
- **B4 — mã lỗi field** (`422 validation_failed`, `errors[]`): `name` `required` / `too_long`; `participants[i].name` `required` / `too_long`, `participants[i].gender` `invalid` (giới tính đọc như chuỗi để báo đúng dòng; thiếu hẳn key → `body invalid`); `participants` `count_out_of_range` (0 hoặc > 500 một lần), `participants` `too_many` (vượt 2.000 người của chương trình, kiểm dưới khóa); `description` `too_long`; `eligible` `invalid`; `quantity` `out_of_range` (ngoài 1–1000); `prizes` `too_many` (đã có 50 giải); `body` `invalid` (JSON hỏng, thiếu key, key lạ — mọi thân là `deny_unknown_fields`). Cả lô người lỗi thì **không thêm ai**, mọi dòng lỗi được liệt kê.
- **B5 — thứ tự kiểm:** quyền `lucky_draw.manage` trước mọi thứ (F17: KTV / quản lý luôn `403`, kể cả id sai dạng), rồi id (`400 path.invalid` khi không phải UUID), `If-Match` / `Idempotency-Key`, thân; trong transaction: khóa chương trình (`404`), tìm người / giải / kết quả **trong chương trình đó** (`404`), version (`412`), luật nghiệp vụ (`409` / `422`). Sửa người đã có kết quả với version đúng → `409 lucky_draw.participant_drawn`; sửa giải đổi `eligible` khi đã có kết quả (kể cả đã trả) → `409 lucky_draw.prize_drawn`; `quantity` < số đã trao → `422 lucky_draw.quantity_below_awarded`. Quay: giải hết suất → `409 lucky_draw.prize_exhausted` (kiểm trước), không còn ai → `409 lucky_draw.no_eligible`.
- **B6 — FK kết quả → giải / người là `RESTRICT` như §2**, xóa chương trình vẫn cascade cả cây (kiểm RESTRICT chạy sau mọi cascade của cùng câu `DELETE`, đã thử bằng test). Kết quả: `app_rw` không có `DELETE`; trigger chặn `DELETE` không đến từ cascade.
- **B7 — giới hạn thân 256 KB** chỉ cho `POST /lucky-draws/{id}/participants`: `app::middleware` thay `tower_http::RequestBodyLimitLayer` (một mức cho mọi route) bằng `limit_body` theo route (`HttpPolicy::larger_bodies`, mẫu path `{…}` = một đoạn); route khác giữ 64 KB. Vượt → `413 payload_too_large`. 500 tên 120 ký tự 3 byte ≈ 190 KB.
- **B8 — audit** (mọi dòng `entity_type = 'lucky_draw'`, `entity_id` = chương trình): `create` `{name}`; `rename` `{name}` → `{name}`; `delete` before `{name, participants, prizes, results}` (số đếm, không tên người); `participants_add` `{added, total}` (không tên người); `participant_update` / `participant_delete` `{participant_id, name, gender}`; `prize_create` / `prize_update` / `prize_delete` `{prize_id, name, description, eligible, quantity}`; `spin` / `forfeit` `{result_id, seq, prize_id, prize, participant_id, winner}`. Nhật ký thao tác: nhóm mới `lucky_draw` ("Vòng quay may mắn"), chủ thể `lucky_draw` = tên chương trình (đã xóa thì tên trong dòng `lucky_draw.delete`), `detail` = tên người (sửa / xóa người, quay, trả giải) hoặc tên giải (thêm / sửa / xóa giải); `RecordTrace` → `null` cho cả 11.
- **B9 — `lucky_draw.manage`**: owner của mọi spa + mẫu owner (0055); **không** vào `OWNER_ONLY` / `OWNER_FLOOR` của console (operator vẫn có thể cấp cho quản lý nếu EM muốn; mặc định chỉ chủ spa). Console nhận tên này vì danh mục permission sinh từ `Permission::ALL` (không `422 unknown_permission`).
- **B11 — jiff ([ADR-0026](../decisions/ADR-0026-jiff-instead-of-chrono.md)):** sau khi gộp nhánh bỏ `chrono`, crate dùng `jiff::Timestamp` (`created_at`, `drawn_at`, `forfeited_at`), `sqlx.toml` giống kernel, cột nullable qua `jiff_sqlx::Timestamp`; mọi instant serialize qua `kernel::time::rfc3339` nên chữ trên dây không đổi (shape §3 giữ nguyên).
- **B10 — một replay sau khi chương trình đã xóa** → `404` (không có gì để trả). Bản ghi idempotency của lượt quay chỉ giữ `{result_id}`.

**Số đo** (local, build debug, router chạy trong process, Postgres Docker; `cargo test -p app --test lucky_draw bench_a_full_draw -- --ignored --nocapture`): chương trình 2.000 người + 50 giải + 1.000 kết quả: `GET /lucky-draws/{id}` p95 **71,6 ms** (ngân sách 150 ms), quay p95 **77,8 ms** (ngân sách 100 ms; gồm cả đọc lại toàn bộ chương trình để trả `draw`). `GET` = 4 câu SQL theo index `(tenant_id, draw_id, …)`; quay = claim idempotency + khóa + giải + nhóm đủ điều kiện + ghi kết quả + version + audit + 3 câu đọc lại.

- **Web, vòng quay luôn quay + popup (2026-11-28, EM):** bỏ "không xoay" khi reduced motion, bỏ vòng lặp chờ / đọc góc / quay về; một transition như bản HTML, lượt đáp khi vòng dừng + có câu trả lời; người trúng chỉ ở popup giữa màn (không còn thẻ kết quả dưới vòng); danh sách trúng thưởng nhóm theo giải, mỗi người một thẻ ([design §3, §3b, §4](../design/lucky-draw.md)).
- **Web (2026-11-28):** lệch + số đo ở [design/lucky-draw §8](../design/lucky-draw.md): hai cột theo bề rộng nội dung ≥ 880px; không `screens.ts` / `ROUTE_PRELOAD` (gate sẽ cộng chunk màn vào đường `/vong-quay`); hai route một chunk; cache qua `putDraw` không lùi `version` (câu trả lời quay được giữ tới khi vòng dừng); vòng lặp quay xoay ảnh bên trong với keyframe hằng, giảm tốc là transition của đĩa; token mới `--texture-wheel-fade` che mép quầng chữ nhật của ảnh khung.
